#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import concurrent.futures
import hashlib
import http.client
import io
import json
import mimetypes
import os
import re
import sys
import tarfile
import time
import xml.etree.ElementTree as ET
import zipfile
from contextlib import closing
from dataclasses import dataclass
from typing import BinaryIO
from urllib import error, parse, request
from uuid import UUID, uuid4

try:
    import tomllib  # type: ignore[attr-defined]
except ModuleNotFoundError:  # pragma: no cover - exercised on Python < 3.11
    tomllib = None


CHECKSUM_SUFFIXES = (".sha1", ".sha256", ".sha512", ".md5")
DEFAULT_TIMEOUT_SECONDS = 60
DEFAULT_RETRIES = 3
DEFAULT_RETRY_BACKOFF_SECONDS = 1.0
DEFAULT_CONTENT_TYPE = "application/octet-stream"
DEFAULT_DEB_ARCHITECTURES = ("amd64", "all")
RETRYABLE_STATUS_CODES = {408, 425, 429, 500, 502, 503, 504}
USER_AGENT = "pkgly-artifactory-to-pkgly/0.3"
SUPPORTED_REPO_TYPES = {"local"}
RAW_COPY_PACKAGE_TYPES = {"composer", "helm", "maven", "pypi"}
SUPPORTED_PACKAGE_TYPES = {
    "cargo",
    "composer",
    "deb",
    "docker",
    "gems",
    "go",
    "helm",
    "maven",
    "npm",
    "nuget",
    "pypi",
}
PACKAGE_TYPE_ALIASES = {
    "composer": "composer",
    "cargo": "cargo",
    "deb": "deb",
    "docker": "docker",
    "gems": "gems",
    "go": "go",
    "gradle": "maven",
    "helm": "helm",
    "ivy": "maven",
    "maven": "maven",
    "npm": "npm",
    "nuget": "nuget",
    "pypi": "pypi",
    "sbt": "maven",
}
PACKAGE_TYPE_TO_PKGLY_TYPE = {
    "cargo": "cargo",
    "composer": "php",
    "deb": "deb",
    "docker": "docker",
    "gems": "ruby",
    "go": "go",
    "helm": "helm",
    "maven": "maven",
    "npm": "npm",
    "nuget": "nuget",
    "pypi": "python",
}
CONTENT_TYPE_OVERRIDES = {
    ".crate": "application/x-tar",
    ".deb": "application/vnd.debian.binary-package",
    ".ear": "application/java-archive",
    ".gem": "application/octet-stream",
    ".jar": "application/java-archive",
    ".mod": "text/plain",
    ".nupkg": "application/octet-stream",
    ".pom": "application/xml",
    ".prov": "application/pgp-signature",
    ".snupkg": "application/octet-stream",
    ".war": "application/java-archive",
    ".whl": "application/zip",
    ".xml": "application/xml",
}
DOCKER_ACCEPT_MEDIA_TYPES = (
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
    "application/vnd.docker.distribution.manifest.v2+json",
)
DOCKER_SCHEMA1_MEDIA_TYPES = {
    "application/vnd.docker.distribution.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v1+prettyjws",
}


class HttpStatusError(RuntimeError):
    def __init__(self, status_code: int, body: str) -> None:
        super().__init__(f"HTTP {status_code}: {body}")
        self.status_code = status_code
        self.body = body


@dataclass(frozen=True)
class RepositoryDescriptor:
    key: str
    package_type: str
    repo_type: str


@dataclass(frozen=True)
class ArtifactEntry:
    path: str
    size: int


@dataclass(frozen=True)
class RepositoryMigrationResult:
    repository_key: str
    package_type: str
    repo_type: str
    status: str
    discovered: int = 0
    skipped_filtered: int = 0
    skipped_existing: int = 0
    skipped_noncanonical: int = 0
    skipped_unsupported_artifacts: int = 0
    transferred: int = 0
    dry_run: int = 0
    error: str = ""


@dataclass(frozen=True)
class ParsedNpmPackage:
    package_name: str
    version: str
    tarball_path: str
    publish_payload: dict


@dataclass(frozen=True)
class ParsedNugetPackage:
    package_id: str
    version: str
    target_path: str


@dataclass(frozen=True)
class ParsedCargoPackage:
    crate_name: str
    version: str
    target_path: str
    payload_bytes: bytes


@dataclass(frozen=True)
class ParsedDebPackage:
    package_name: str
    version: str
    architecture: str
    target_path: str


@dataclass
class GoArtifactGroup:
    zip_entry: ArtifactEntry | None = None
    mod_entry: ArtifactEntry | None = None
    info_entry: ArtifactEntry | None = None


def normalize_base_url(url: str) -> str:
    return url.rstrip("/")


def normalize_path(path: str) -> str:
    return path.strip("/")


def quote_path(path: str) -> str:
    return parse.quote(path, safe="/-._~:@+")


def build_auth_header(token: str | None, username: str | None, password: str | None) -> str | None:
    if token:
        return f"Bearer {token}"
    if username is None and password is None:
        return None
    if username is None or password is None:
        raise ValueError("username and password must be provided together")
    credentials = f"{username}:{password}".encode("utf-8")
    encoded = base64.b64encode(credentials).decode("ascii")
    return f"Basic {encoded}"


def resolve_argument(value: str | None, env_name: str) -> str | None:
    if value is not None:
        return value
    env_value = os.environ.get(env_name)
    return env_value or None


def validate_uuid(value: str, argument_name: str) -> str:
    try:
        UUID(value)
    except ValueError as exc:
        raise RuntimeError(f"{argument_name} must be a valid UUID") from exc
    return value


def map_artifactory_package_type(package_type: str) -> str:
    return PACKAGE_TYPE_ALIASES.get(package_type.lower(), package_type.lower())


def is_retryable_exception(exc: Exception) -> bool:
    if isinstance(exc, HttpStatusError):
        return exc.status_code in RETRYABLE_STATUS_CODES
    if isinstance(exc, error.HTTPError):
        return exc.code in RETRYABLE_STATUS_CODES
    return isinstance(exc, (TimeoutError, error.URLError, http.client.HTTPException, OSError))


def retry_operation(
    description: str,
    operation,
    *,
    retries: int,
    backoff_seconds: float,
    sleep=time.sleep,
):
    for attempt in range(retries + 1):
        try:
            return operation()
        except Exception as exc:
            if attempt >= retries or not is_retryable_exception(exc):
                raise
            delay = backoff_seconds * (2**attempt)
            print(
                f"Retrying {description} after {delay:.1f}s due to: {exc}",
                file=sys.stderr,
            )
            sleep(delay)


def guess_content_type(path: str) -> str:
    lowered = path.lower()
    for suffix, content_type in CONTENT_TYPE_OVERRIDES.items():
        if lowered.endswith(suffix):
            return content_type
    guessed, _ = mimetypes.guess_type(path)
    return guessed or DEFAULT_CONTENT_TYPE


def should_copy_path(package_type: str, path: str) -> bool:
    return resolve_target_path(package_type, path) is not None


def _normalize_python_package_name(name: str) -> str:
    return name.lower().replace("_", "-").replace(".", "-")


def _parse_python_filename(filename: str) -> tuple[str, str] | None:
    if filename.endswith(".whl"):
        stem = filename[:-4]
        parts = stem.split("-")
        if len(parts) not in {5, 6}:
            return None
        return parts[0], parts[1]

    suffixes = (".tar.gz", ".tar.bz2", ".tgz", ".zip", ".egg", ".tar")
    for suffix in suffixes:
        if filename.endswith(suffix):
            stem = filename[: -len(suffix)]
            if suffix == ".egg":
                parts = stem.rsplit("-", 2)
                if len(parts) != 3:
                    return None
                return parts[0], parts[1]
            parts = stem.rsplit("-", 1)
            if len(parts) != 2:
                return None
            return parts[0], parts[1]
    return None


def _resolve_python_target_path(path: str) -> str | None:
    normalized = normalize_path(path)
    if not normalized or normalized.lower().endswith(CHECKSUM_SUFFIXES):
        return None

    parts = normalized.split("/")
    filename = parts[-1]
    parsed_filename = _parse_python_filename(filename)
    if parsed_filename is None:
        return None
    package, version = parsed_filename

    if len(parts) >= 3:
        path_package = parts[-3]
        path_version = parts[-2]
        if _normalize_python_package_name(path_package) == _normalize_python_package_name(
            package
        ) and path_version == version:
            return "/".join([path_package, path_version, filename])

    return "/".join([package, version, filename])


def _resolve_helm_target_path(path: str) -> str | None:
    normalized = normalize_path(path)
    if not normalized or normalized.lower().endswith(CHECKSUM_SUFFIXES):
        return None
    filename = normalized.rsplit("/", 1)[-1]
    if filename.endswith(".tgz.prov"):
        stem = filename[: -len(".tgz.prov")]
        suffix = ".tgz.prov"
    elif filename.endswith(".tgz"):
        stem = filename[: -len(".tgz")]
        suffix = ".tgz"
    else:
        return None
    name, sep, version = stem.rpartition("-")
    if not sep or not name or not version:
        return None
    return f"charts/{name}/{name}-{version}{suffix}"


def _resolve_php_target_path(path: str) -> str | None:
    normalized = normalize_path(path)
    if not normalized or normalized.lower().endswith(CHECKSUM_SUFFIXES):
        return None
    parts = normalized.split("/")
    if parts[0] == "dist":
        parts = parts[1:]
    if len(parts) < 3:
        return None
    vendor = parts[0]
    package = parts[1]
    filename = parts[-1]
    if not filename.endswith(".zip"):
        return None
    return f"dist/{vendor}/{package}/{filename}"


def resolve_target_path(package_type: str, path: str) -> str | None:
    normalized = normalize_path(path)
    canonical_type = map_artifactory_package_type(package_type)
    if not normalized:
        return None
    if canonical_type == "maven":
        if normalized.lower().endswith(CHECKSUM_SUFFIXES):
            return None
        return normalized
    if canonical_type == "helm":
        return _resolve_helm_target_path(normalized)
    if canonical_type == "composer":
        return _resolve_php_target_path(normalized)
    if canonical_type == "pypi":
        return _resolve_python_target_path(normalized)
    return None


def parse_repositories_response(payload: list[dict]) -> list[RepositoryDescriptor]:
    repositories = []
    for item in payload:
        repositories.append(
            RepositoryDescriptor(
                key=str(item["key"]),
                package_type=str(item.get("packageType", "")).lower(),
                repo_type=str(item.get("repoType", "")).lower(),
            )
        )
    return repositories


def parse_artifactory_file_list(payload: dict) -> list[ArtifactEntry]:
    entries = []
    for item in payload.get("files", []):
        item_type = str(item.get("type", "file")).lower()
        if item_type == "folder":
            continue
        path = str(item.get("path", item.get("uri", ""))).lstrip("/")
        entries.append(ArtifactEntry(path=path, size=int(item.get("size", 0))))
    return entries


def select_repositories(
    available: list[RepositoryDescriptor],
    requested_names: list[str],
    all_repositories: bool,
) -> tuple[list[RepositoryDescriptor], list[str]]:
    by_key = {repo.key: repo for repo in available}
    if all_repositories:
        return list(available), []
    selected = []
    missing = []
    for name in requested_names:
        repo = by_key.get(name)
        if repo is None:
            missing.append(name)
            continue
        selected.append(repo)
    return selected, missing


def _request_bytes(
    url: str,
    *,
    method: str,
    auth_header: str | None,
    timeout: int,
    payload: bytes | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[int, dict[str, str], bytes]:
    request_headers = {"User-Agent": USER_AGENT}
    if auth_header:
        request_headers["Authorization"] = auth_header
    if headers:
        request_headers.update(headers)
    req = request.Request(url, data=payload, headers=request_headers, method=method)
    try:
        with request.urlopen(req, timeout=timeout) as response:
            return response.status, dict(response.headers.items()), response.read()
    except error.HTTPError as exc:
        body = exc.read()
        return exc.code, dict(exc.headers.items()), body


def _json_request(
    url: str,
    *,
    method: str,
    auth_header: str | None,
    timeout: int,
    payload: dict | None = None,
    headers: dict[str, str] | None = None,
) -> dict | list:
    request_headers = {"Accept": "application/json"}
    if headers:
        request_headers.update(headers)
    data = None
    if payload is not None:
        request_headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
    status, _, body = _request_bytes(
        url,
        method=method,
        auth_header=auth_header,
        timeout=timeout,
        payload=data,
        headers=request_headers,
    )
    if status < 200 or status >= 300:
        raise HttpStatusError(status, body.decode("utf-8", errors="replace"))
    if not body:
        return {}
    return json.loads(body.decode("utf-8"))


def _head_request(
    url: str,
    *,
    auth_header: str | None,
    timeout: int,
    headers: dict[str, str] | None = None,
) -> int:
    status, _, _ = _request_bytes(
        url,
        method="HEAD",
        auth_header=auth_header,
        timeout=timeout,
        headers=headers,
    )
    return status


def _stream_put(
    url: str,
    *,
    auth_header: str | None,
    content_type: str,
    timeout: int,
    stream: BinaryIO,
    size: int,
    headers: dict[str, str] | None = None,
) -> None:
    parsed = parse.urlsplit(url)
    connection_class = (
        http.client.HTTPSConnection if parsed.scheme == "https" else http.client.HTTPConnection
    )
    connection = connection_class(parsed.hostname, parsed.port, timeout=timeout)
    request_path = parsed.path or "/"
    if parsed.query:
        request_path = f"{request_path}?{parsed.query}"

    try:
        connection.putrequest("PUT", request_path)
        connection.putheader("User-Agent", USER_AGENT)
        connection.putheader("Content-Length", str(size))
        connection.putheader("Content-Type", content_type)
        if auth_header:
            connection.putheader("Authorization", auth_header)
        for key, value in (headers or {}).items():
            connection.putheader(key, value)
        connection.endheaders()

        while True:
            chunk = stream.read(64 * 1024)
            if not chunk:
                break
            connection.send(chunk)

        response = connection.getresponse()
        body = response.read().decode("utf-8", errors="replace")
    finally:
        connection.close()

    if response.status < 200 or response.status >= 300:
        raise HttpStatusError(response.status, body)


def _encode_multipart(fields: dict[str, str], files: list[tuple[str, str, str, bytes]]) -> tuple[str, bytes]:
    boundary = f"----pkgly-migration-{uuid4().hex}"
    body = bytearray()
    for name, value in fields.items():
        body.extend(f"--{boundary}\r\n".encode("ascii"))
        body.extend(
            f'Content-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode("utf-8")
        )
    for field_name, filename, content_type, content in files:
        body.extend(f"--{boundary}\r\n".encode("ascii"))
        body.extend(
            (
                f'Content-Disposition: form-data; name="{field_name}"; '
                f'filename="{filename}"\r\n'
            ).encode("utf-8")
        )
        body.extend(f"Content-Type: {content_type}\r\n\r\n".encode("ascii"))
        body.extend(content)
        body.extend(b"\r\n")
    body.extend(f"--{boundary}--\r\n".encode("ascii"))
    return f"multipart/form-data; boundary={boundary}", bytes(body)


def _join_location(base_url: str, location: str) -> str:
    return parse.urljoin(f"{normalize_base_url(base_url)}/", location)


def _safe_json_loads(payload: bytes) -> dict:
    return json.loads(payload.decode("utf-8"))


def _version_sort_key(value: str) -> tuple:
    parts = re.split(r"([0-9]+)", value)
    key = []
    for part in parts:
        if not part:
            continue
        if part.isdigit():
            key.append((0, int(part)))
        else:
            key.append((1, part))
    return tuple(key)


class ArtifactoryClient:
    def __init__(self, *, base_url: str, auth_header: str | None, timeout: int) -> None:
        self.base_url = normalize_base_url(base_url)
        self.auth_header = auth_header
        self.timeout = timeout

    def list_repositories(self) -> list[RepositoryDescriptor]:
        payload = _json_request(
            f"{self.base_url}/artifactory/api/repositories",
            method="GET",
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        return parse_repositories_response(payload)

    def list_files(self, repository_key: str, path_prefix: str = "") -> list[ArtifactEntry]:
        prefix = normalize_path(path_prefix)
        base = f"{self.base_url}/artifactory/api/repo/{parse.quote(repository_key)}/list"
        if prefix:
            base = f"{base}/{quote_path(prefix)}"
        else:
            base = f"{base}/"
        payload = _json_request(
            f"{base}?deep=1&listFolders=0",
            method="GET",
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        return parse_artifactory_file_list(payload)

    def open_file(self, repository_key: str, path: str):
        quoted = quote_path(normalize_path(path))
        url = f"{self.base_url}/artifactory/{parse.quote(repository_key)}/{quoted}"
        headers = {"User-Agent": USER_AGENT}
        if self.auth_header:
            headers["Authorization"] = self.auth_header
        req = request.Request(url, headers=headers, method="GET")
        return request.urlopen(req, timeout=self.timeout)

    def list_docker_images(self, repository_key: str) -> list[str]:
        payload = _json_request(
            f"{self.base_url}/artifactory/api/docker/{parse.quote(repository_key)}/v2/_catalog",
            method="GET",
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        return list(payload.get("repositories", []))

    def list_docker_tags(self, repository_key: str, image_name: str) -> list[str]:
        payload = _json_request(
            f"{self.base_url}/artifactory/api/docker/{parse.quote(repository_key)}/v2/"
            f"{quote_path(image_name)}/tags/list",
            method="GET",
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        return list(payload.get("tags", []) or [])

    def get_docker_manifest(self, repository_key: str, image_name: str, reference: str) -> tuple[bytes, str]:
        status, headers, body = _request_bytes(
            f"{self.base_url}/artifactory/api/docker/{parse.quote(repository_key)}/v2/"
            f"{quote_path(image_name)}/manifests/{parse.quote(reference, safe=':@')}",
            method="GET",
            auth_header=self.auth_header,
            timeout=self.timeout,
            headers={"Accept": ", ".join(DOCKER_ACCEPT_MEDIA_TYPES)},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, body.decode("utf-8", errors="replace"))
        return body, headers.get("Content-Type", "").split(";", 1)[0].strip()

    def open_docker_blob(self, repository_key: str, image_name: str, digest: str):
        url = (
            f"{self.base_url}/artifactory/api/docker/{parse.quote(repository_key)}/v2/"
            f"{quote_path(image_name)}/blobs/{parse.quote(digest, safe=':')}"
        )
        headers = {"User-Agent": USER_AGENT}
        if self.auth_header:
            headers["Authorization"] = self.auth_header
        req = request.Request(url, headers=headers, method="GET")
        return request.urlopen(req, timeout=self.timeout)


class PkglyClient:
    def __init__(self, *, base_url: str, auth_header: str | None, timeout: int) -> None:
        self.base_url = normalize_base_url(base_url)
        self.auth_header = auth_header
        self.timeout = timeout

    def repository_exists(self, storage_name: str, repository_name: str) -> bool:
        try:
            _json_request(
                (
                    f"{self.base_url}/api/repository/find-id/"
                    f"{parse.quote(storage_name)}/{parse.quote(repository_name)}"
                ),
                method="GET",
                auth_header=self.auth_header,
                timeout=self.timeout,
            )
            return True
        except HttpStatusError as exc:
            if exc.status_code == 404:
                return False
            raise

    def create_repository(
        self,
        storage_name: str,
        repository_name: str,
        package_type: str,
        package_options: dict | None = None,
    ) -> None:
        if package_type not in PACKAGE_TYPE_TO_PKGLY_TYPE.values():
            raise RuntimeError(f"unsupported package type for target creation: {package_type}")
        config = {"type": "Hosted"}
        if package_options:
            config.update(package_options)
        payload = {
            "name": repository_name,
            "storage_name": storage_name,
            "configs": {
                package_type: config,
            },
        }
        _json_request(
            f"{self.base_url}/api/repository/new/{parse.quote(package_type)}",
            method="POST",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=payload,
        )

    def artifact_exists(self, storage_name: str, repository_name: str, relative_path: str) -> bool:
        status = _head_request(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(relative_path)}"
            ),
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        if status == 200:
            return True
        if status == 404:
            return False
        raise RuntimeError(f"Pkgly HEAD check failed with HTTP {status}")

    def upload_file(
        self,
        storage_name: str,
        repository_name: str,
        relative_path: str,
        stream: BinaryIO,
        size: int,
    ) -> None:
        _stream_put(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(relative_path)}"
            ),
            auth_header=self.auth_header,
            content_type=guess_content_type(relative_path),
            timeout=self.timeout,
            stream=stream,
            size=size,
        )

    def publish_npm_package(
        self,
        storage_name: str,
        repository_name: str,
        package_name: str,
        version: str,
        tarball_path: str,
        payload: dict,
    ) -> None:
        del version, tarball_path
        body = json.dumps(payload).encode("utf-8")
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(package_name)}"
            ),
            method="PUT",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=body,
            headers={
                "Content-Type": "application/json",
                "npm-command": "publish",
            },
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def publish_nuget_package(
        self,
        storage_name: str,
        repository_name: str,
        package_id: str,
        version: str,
        filename: str,
        package_bytes: bytes,
    ) -> None:
        del package_id, version
        content_type, body = _encode_multipart(
            {},
            [("package", filename, guess_content_type(filename), package_bytes)],
        )
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/api/v2/package"
            ),
            method="PUT",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=body,
            headers={"Content-Type": content_type},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def publish_ruby_gem(
        self,
        storage_name: str,
        repository_name: str,
        filename: str,
        gem_bytes: bytes,
    ) -> None:
        del filename
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/api/v1/gems"
            ),
            method="POST",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=gem_bytes,
            headers={"Content-Type": "application/octet-stream"},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def publish_cargo_package(
        self,
        storage_name: str,
        repository_name: str,
        crate_name: str,
        version: str,
        payload_bytes: bytes,
    ) -> None:
        del crate_name, version
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/api/v1/crates/new"
            ),
            method="PUT",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=payload_bytes,
            headers={"Content-Type": "application/octet-stream"},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def upload_go_module(
        self,
        storage_name: str,
        repository_name: str,
        module_name: str,
        version: str,
        module_zip: bytes,
        go_mod: bytes | None,
        info_json: bytes | None,
    ) -> None:
        fields = {
            "version": version,
            "module_name": module_name,
        }
        files = [("module", f"{module_name.rsplit('/', 1)[-1]}-{version}.zip", "application/zip", module_zip)]
        if go_mod is not None:
            files.append(("gomod", "go.mod", "text/plain", go_mod))
        if info_json is not None:
            files.append(("info", f"{version}.info", "application/json", info_json))
        content_type, body = _encode_multipart(fields, files)
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/upload"
            ),
            method="POST",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=body,
            headers={"Content-Type": content_type},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def upload_deb_package(
        self,
        storage_name: str,
        repository_name: str,
        distribution: str,
        component: str,
        filename: str,
        package_bytes: bytes,
    ) -> None:
        content_type, body = _encode_multipart(
            {
                "distribution": distribution,
                "component": component,
            },
            [("package", filename, guess_content_type(filename), package_bytes)],
        )
        status, _, response_body = _request_bytes(
            (
                f"{self.base_url}/repositories/"
                f"{parse.quote(storage_name)}/{parse.quote(repository_name)}"
            ),
            method="POST",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=body,
            headers={"Content-Type": content_type},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, response_body.decode("utf-8", errors="replace"))

    def docker_blob_exists(self, storage_name: str, repository_name: str, image_name: str, digest: str) -> bool:
        status = _head_request(
            (
                f"{self.base_url}/v2/{parse.quote(storage_name)}/{parse.quote(repository_name)}/"
                f"{quote_path(image_name)}/blobs/{parse.quote(digest, safe=':')}"
            ),
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        if status == 200:
            return True
        if status == 404:
            return False
        raise RuntimeError(f"Pkgly Docker blob lookup failed with HTTP {status}")

    def upload_docker_blob(
        self,
        storage_name: str,
        repository_name: str,
        image_name: str,
        digest: str,
        blob_bytes: bytes,
    ) -> None:
        start_url = (
            f"{self.base_url}/v2/{parse.quote(storage_name)}/{parse.quote(repository_name)}/"
            f"{quote_path(image_name)}/blobs/uploads/"
        )
        status, headers, body = _request_bytes(
            start_url,
            method="POST",
            auth_header=self.auth_header,
            timeout=self.timeout,
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, body.decode("utf-8", errors="replace"))
        upload_url = _join_location(self.base_url, headers.get("Location", ""))
        status, headers, body = _request_bytes(
            upload_url,
            method="PATCH",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=blob_bytes,
            headers={"Content-Type": "application/octet-stream"},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, body.decode("utf-8", errors="replace"))
        finalize_url = _join_location(self.base_url, headers.get("Location", upload_url))
        separator = "&" if parse.urlsplit(finalize_url).query else "?"
        finalize_url = f"{finalize_url}{separator}digest={parse.quote(digest, safe=':')}"
        status, _, body = _request_bytes(
            finalize_url,
            method="PUT",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=b"",
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, body.decode("utf-8", errors="replace"))

    def docker_manifest_exists(
        self,
        storage_name: str,
        repository_name: str,
        image_name: str,
        reference: str,
    ) -> bool:
        status = _head_request(
            (
                f"{self.base_url}/v2/{parse.quote(storage_name)}/{parse.quote(repository_name)}/"
                f"{quote_path(image_name)}/manifests/{parse.quote(reference, safe=':@')}"
            ),
            auth_header=self.auth_header,
            timeout=self.timeout,
            headers={"Accept": ", ".join(DOCKER_ACCEPT_MEDIA_TYPES)},
        )
        if status == 200:
            return True
        if status == 404:
            return False
        raise RuntimeError(f"Pkgly Docker manifest lookup failed with HTTP {status}")

    def upload_docker_manifest(
        self,
        storage_name: str,
        repository_name: str,
        image_name: str,
        reference: str,
        media_type: str,
        manifest_bytes: bytes,
    ) -> None:
        status, _, body = _request_bytes(
            (
                f"{self.base_url}/v2/{parse.quote(storage_name)}/{parse.quote(repository_name)}/"
                f"{quote_path(image_name)}/manifests/{parse.quote(reference, safe=':@')}"
            ),
            method="PUT",
            auth_header=self.auth_header,
            timeout=self.timeout,
            payload=manifest_bytes,
            headers={"Content-Type": media_type},
        )
        if status < 200 or status >= 300:
            raise HttpStatusError(status, body.decode("utf-8", errors="replace"))


def _read_all(stream: BinaryIO) -> bytes:
    return stream.read()


def _read_artifactory_file_bytes(
    artifactory: ArtifactoryClient,
    repository_key: str,
    path: str,
) -> bytes:
    with closing(artifactory.open_file(repository_key, path)) as stream:
        return stream.read()


def _read_artifactory_docker_blob_bytes(
    artifactory: ArtifactoryClient,
    repository_key: str,
    image_name: str,
    digest: str,
) -> bytes:
    with closing(artifactory.open_docker_blob(repository_key, image_name, digest)) as stream:
        return stream.read()


def _find_archive_member(
    data: bytes,
    *,
    suffixes: tuple[str, ...] | None = None,
    exact_names: tuple[str, ...] | None = None,
) -> bytes:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive.getmembers():
            member_name = member.name.lstrip("./")
            if exact_names and member_name in exact_names:
                extracted = archive.extractfile(member)
                if extracted is None:
                    break
                return extracted.read()
            if suffixes and member_name.endswith(suffixes):
                extracted = archive.extractfile(member)
                if extracted is None:
                    break
                return extracted.read()
    raise RuntimeError("required archive member not found")


def _split_toml_items(value: str, delimiter: str = ",") -> list[str]:
    items = []
    current = []
    depth = 0
    quote = None
    for character in value:
        if quote is not None:
            current.append(character)
            if character == quote:
                quote = None
            continue
        if character in {"'", '"'}:
            quote = character
            current.append(character)
            continue
        if character in "[{":
            depth += 1
        elif character in "]}":
            depth -= 1
        if character == delimiter and depth == 0:
            items.append("".join(current).strip())
            current = []
            continue
        current.append(character)
    if current:
        items.append("".join(current).strip())
    return [item for item in items if item]


def _strip_toml_comment(line: str) -> str:
    result = []
    quote = None
    for character in line:
        if quote is not None:
            result.append(character)
            if character == quote:
                quote = None
            continue
        if character in {"'", '"'}:
            quote = character
            result.append(character)
            continue
        if character == "#":
            break
        result.append(character)
    return "".join(result).strip()


def _parse_toml_key_path(header: str) -> list[str]:
    keys = []
    current = []
    quote = None
    for character in header:
        if quote is not None:
            if character == quote:
                quote = None
            else:
                current.append(character)
            continue
        if character in {"'", '"'}:
            quote = character
            continue
        if character == ".":
            keys.append("".join(current).strip())
            current = []
            continue
        current.append(character)
    keys.append("".join(current).strip())
    return [key for key in keys if key]


def _parse_toml_value(raw_value: str):
    raw_value = raw_value.strip()
    if raw_value.startswith('"') and raw_value.endswith('"'):
        return raw_value[1:-1]
    if raw_value.startswith("'") and raw_value.endswith("'"):
        return raw_value[1:-1]
    if raw_value == "true":
        return True
    if raw_value == "false":
        return False
    if raw_value.startswith("[") and raw_value.endswith("]"):
        inner = raw_value[1:-1].strip()
        if not inner:
            return []
        return [_parse_toml_value(item) for item in _split_toml_items(inner)]
    if raw_value.startswith("{") and raw_value.endswith("}"):
        inner = raw_value[1:-1].strip()
        if not inner:
            return {}
        parsed = {}
        for item in _split_toml_items(inner):
            key, _, value = item.partition("=")
            parsed[key.strip()] = _parse_toml_value(value)
        return parsed
    if raw_value.isdigit():
        return int(raw_value)
    return raw_value


def _load_toml(payload: bytes) -> dict:
    if tomllib is not None:
        return tomllib.loads(payload.decode("utf-8"))

    root: dict = {}
    current = root
    for raw_line in payload.decode("utf-8").splitlines():
        line = _strip_toml_comment(raw_line)
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            current = root
            for key in _parse_toml_key_path(line[1:-1].strip()):
                current = current.setdefault(key, {})
            continue
        key, separator, value = line.partition("=")
        if not separator:
            raise RuntimeError(f"unsupported TOML syntax: {raw_line}")
        current[key.strip()] = _parse_toml_value(value)
    return root


def parse_npm_package_bytes(
    payload: bytes,
    *,
    pkgly_base_url: str,
    storage_name: str,
    repository_name: str,
) -> ParsedNpmPackage:
    package_json_bytes = _find_archive_member(
        payload,
        exact_names=("package/package.json", "package.json"),
    )
    package_json = json.loads(package_json_bytes.decode("utf-8"))
    package_name = str(package_json["name"])
    version = str(package_json["version"])
    attachment_name = package_name.rsplit("/", 1)[-1]
    tarball_filename = f"{attachment_name}-{version}.tgz"
    tarball_path = f"{package_name}/-/{tarball_filename}"
    tarball_url = (
        f"{normalize_base_url(pkgly_base_url)}/repositories/"
        f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(tarball_path)}"
    )

    publish_version = dict(package_json)
    publish_version.update(
        {
            "name": package_name,
            "version": version,
            "_id": f"{package_name}@{version}",
            "readme": package_json.get("readme", ""),
            "readmeFilename": package_json.get("readmeFilename", ""),
            "_nodeVersion": package_json.get("_nodeVersion", "migration"),
            "_npmVersion": package_json.get("_npmVersion", "migration"),
            "dist": {
                "shasum": hashlib.sha1(payload).hexdigest(),
                "integrity": "sha512-"
                + base64.b64encode(hashlib.sha512(payload).digest()).decode("ascii"),
                "tarball": tarball_url,
            },
        }
    )
    publish_payload = {
        "name": package_name,
        "versions": {
            version: publish_version,
        },
        "_attachments": {
            tarball_path: {
                "content_type": "application/octet-stream",
                "data": base64.b64encode(payload).decode("ascii"),
                "length": len(payload),
            }
        },
    }
    return ParsedNpmPackage(
        package_name=package_name,
        version=version,
        tarball_path=tarball_path,
        publish_payload=publish_payload,
    )


def parse_nuget_package_bytes(filename: str, payload: bytes) -> ParsedNugetPackage:
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        nuspec_name = next((name for name in archive.namelist() if name.endswith(".nuspec")), None)
        if nuspec_name is None:
            raise RuntimeError(f"{filename} does not contain a .nuspec file")
        root = ET.fromstring(archive.read(nuspec_name))
    package_id = root.findtext(".//{*}id")
    version = root.findtext(".//{*}version")
    if not package_id or not version:
        raise RuntimeError(f"{filename} is missing package id or version")
    lower_id = package_id.lower()
    lower_version = version.lower()
    return ParsedNugetPackage(
        package_id=package_id,
        version=version,
        target_path=f"v3/flatcontainer/{lower_id}/{lower_version}/{lower_id}.{lower_version}.nupkg",
    )


def _normalize_cargo_version_spec(value) -> str | None:
    if value is None:
        return None
    if isinstance(value, str):
        return value
    if isinstance(value, dict):
        version = value.get("version")
        return str(version) if version is not None else None
    return None


def _cargo_dependency_from_value(name: str, value, *, kind: str | None, target: str | None) -> dict:
    if isinstance(value, str):
        return {
            "name": name,
            "vers": value,
            "optional": False,
            "default_features": True,
            "features": [],
            "target": target,
            "kind": kind,
            "registry": None,
            "package": None,
        }
    if not isinstance(value, dict):
        raise RuntimeError(f"unsupported Cargo dependency declaration for {name}")
    package_name = value.get("package")
    return {
        "name": name,
        "vers": _normalize_cargo_version_spec(value),
        "optional": bool(value.get("optional", False)),
        "default_features": bool(value.get("default-features", True)),
        "features": list(value.get("features", [])),
        "target": target,
        "kind": kind,
        "registry": value.get("registry"),
        "package": package_name,
    }


def _extract_cargo_metadata(payload: bytes) -> dict:
    cargo_toml = _find_archive_member(payload, suffixes=("/Cargo.toml",))
    manifest = _load_toml(cargo_toml)
    package = manifest.get("package")
    if not isinstance(package, dict):
        raise RuntimeError("Cargo.toml is missing [package]")

    dependencies = []
    for table_name, kind in (
        ("dependencies", None),
        ("dev-dependencies", "dev"),
        ("build-dependencies", "build"),
    ):
        section = manifest.get(table_name, {})
        if isinstance(section, dict):
            for name, value in section.items():
                dependencies.append(
                    _cargo_dependency_from_value(name, value, kind=kind, target=None)
                )

    target_tables = manifest.get("target", {})
    if isinstance(target_tables, dict):
        for target_name, target_config in target_tables.items():
            if not isinstance(target_config, dict):
                continue
            for table_name, kind in (
                ("dependencies", None),
                ("dev-dependencies", "dev"),
                ("build-dependencies", "build"),
            ):
                section = target_config.get(table_name, {})
                if not isinstance(section, dict):
                    continue
                for name, value in section.items():
                    dependencies.append(
                        _cargo_dependency_from_value(name, value, kind=kind, target=target_name)
                    )

    return {
        "name": str(package["name"]),
        "vers": str(package["version"]),
        "deps": dependencies,
        "features": manifest.get("features", {}),
        "authors": list(package.get("authors", [])),
        "description": package.get("description"),
        "documentation": package.get("documentation"),
        "homepage": package.get("homepage"),
        "repository": package.get("repository"),
        "keywords": list(package.get("keywords", [])),
        "categories": list(package.get("categories", [])),
        "license": package.get("license"),
        "license_file": package.get("license-file"),
        "readme": package.get("readme"),
        "readme_file": package.get("readme-file"),
        "badges": manifest.get("badges", {}),
        "links": package.get("links"),
    }


def build_cargo_publish_body(payload: bytes) -> bytes:
    metadata = json.dumps(_extract_cargo_metadata(payload), separators=(",", ":")).encode("utf-8")
    return (
        len(metadata).to_bytes(4, "little")
        + metadata
        + len(payload).to_bytes(4, "little")
        + payload
    )


def parse_cargo_publish_body(payload: bytes) -> dict:
    if len(payload) < 8:
        raise RuntimeError("truncated Cargo publish body")
    metadata_len = int.from_bytes(payload[:4], "little")
    metadata_end = 4 + metadata_len
    metadata = json.loads(payload[4:metadata_end].decode("utf-8"))
    crate_len = int.from_bytes(payload[metadata_end : metadata_end + 4], "little")
    crate_archive = payload[metadata_end + 4 : metadata_end + 4 + crate_len]
    return {"metadata": metadata, "crate_archive": crate_archive}


def parse_cargo_package_bytes(filename: str, payload: bytes) -> ParsedCargoPackage:
    metadata = _extract_cargo_metadata(payload)
    crate_name = str(metadata["name"])
    version = str(metadata["vers"])
    normalized = crate_name.lower()
    return ParsedCargoPackage(
        crate_name=crate_name,
        version=version,
        target_path=f"crates/{normalized}/{version}/{normalized}-{version}.crate",
        payload_bytes=build_cargo_publish_body(payload),
    )


def _split_go_artifact_path(path: str) -> tuple[str, str, str] | None:
    match = re.match(r"^(.+)/@v/([^/]+)\.(zip|mod|info)$", normalize_path(path))
    if match is None:
        return None
    return match.group(1), match.group(2), match.group(3)


def group_go_artifacts(entries: list[ArtifactEntry]) -> tuple[dict[tuple[str, str], GoArtifactGroup], int]:
    groups: dict[tuple[str, str], GoArtifactGroup] = {}
    skipped_noncanonical = 0
    for entry in entries:
        parsed = _split_go_artifact_path(entry.path)
        if parsed is None:
            skipped_noncanonical += 1
            continue
        module_name, version, extension = parsed
        group = groups.setdefault((module_name, version), GoArtifactGroup())
        if extension == "zip":
            group.zip_entry = entry
        elif extension == "mod":
            group.mod_entry = entry
        else:
            group.info_entry = entry
    return groups, skipped_noncanonical


def _parse_ar_members(payload: bytes) -> dict[str, bytes]:
    if not payload.startswith(b"!<arch>\n"):
        raise RuntimeError("invalid ar archive")
    offset = 8
    members: dict[str, bytes] = {}
    while offset + 60 <= len(payload):
        header = payload[offset : offset + 60]
        offset += 60
        name = header[:16].decode("utf-8").strip()
        size = int(header[48:58].decode("ascii").strip())
        content = payload[offset : offset + size]
        offset += size
        if size % 2 == 1:
            offset += 1
        members[name.rstrip("/")] = content
    return members


def _parse_deb_control(payload: bytes) -> dict[str, str]:
    members = _parse_ar_members(payload)
    control_name = next((name for name in members if name.startswith("control.tar")), None)
    if control_name is None:
        raise RuntimeError("deb package is missing control.tar.*")
    with tarfile.open(fileobj=io.BytesIO(members[control_name]), mode="r:*") as archive:
        for member in archive.getmembers():
            member_name = member.name.lstrip("./")
            if member_name != "control":
                continue
            extracted = archive.extractfile(member)
            if extracted is None:
                break
            lines = extracted.read().decode("utf-8").splitlines()
            fields: dict[str, str] = {}
            current_key = None
            for line in lines:
                if not line:
                    continue
                if line.startswith(" ") and current_key is not None:
                    fields[current_key] += f"\n{line[1:]}"
                    continue
                key, _, value = line.partition(":")
                current_key = key.strip()
                fields[current_key] = value.strip()
            return fields
    raise RuntimeError("deb package is missing control metadata")


def parse_deb_package_bytes(filename: str, payload: bytes, *, component: str) -> ParsedDebPackage:
    control = _parse_deb_control(payload)
    package_name = control.get("Package")
    version = control.get("Version")
    architecture = control.get("Architecture")
    if not package_name or not version or not architecture:
        raise RuntimeError(f"{filename} is missing Debian control fields")
    first_letter = package_name[0].lower()
    return ParsedDebPackage(
        package_name=package_name,
        version=version,
        architecture=architecture,
        target_path=f"pool/{component}/{first_letter}/{package_name}/{filename}",
    )


def _extract_docker_blob_digests(manifest: dict) -> list[str]:
    digests = []
    config = manifest.get("config")
    if isinstance(config, dict) and config.get("digest"):
        digests.append(str(config["digest"]))
    for layer in manifest.get("layers", []):
        digest = layer.get("digest")
        if digest:
            digests.append(str(digest))
    return digests


def _extract_docker_child_manifests(manifest: dict) -> list[str]:
    digests = []
    for child in manifest.get("manifests", []):
        digest = child.get("digest")
        if digest:
            digests.append(str(digest))
    return digests


def _resolve_package_options(package_options: dict | None) -> dict:
    package_options = package_options or {}
    deb_distribution = package_options.get("deb_distribution", "stable")
    deb_component = package_options.get("deb_component", "main")
    deb_architectures = package_options.get(
        "deb_architectures", list(DEFAULT_DEB_ARCHITECTURES)
    )
    return {
        "deb_distribution": deb_distribution,
        "deb_component": deb_component,
        "deb_architectures": list(deb_architectures),
    }


def _target_repository_options(package_type: str, package_options: dict | None) -> dict | None:
    if package_type != "deb":
        return None
    resolved = _resolve_package_options(package_options)
    return {
        "distributions": [resolved["deb_distribution"]],
        "components": [resolved["deb_component"]],
        "architectures": list(resolved["deb_architectures"]),
    }


def prepare_target_repository(
    *,
    pkgly: PkglyClient,
    storage_name: str,
    repository: RepositoryDescriptor,
    create_targets: bool,
    package_options: dict | None,
) -> None:
    package_type = map_artifactory_package_type(repository.package_type)
    if package_type not in SUPPORTED_PACKAGE_TYPES:
        raise RuntimeError(f"unsupported package type: {repository.package_type}")
    if pkgly.repository_exists(storage_name, repository.key):
        return
    if not create_targets:
        raise RuntimeError(f"target repository {storage_name}/{repository.key} does not exist")
    pkgly.create_repository(
        storage_name,
        repository.key,
        PACKAGE_TYPE_TO_PKGLY_TYPE[package_type],
        _target_repository_options(package_type, package_options),
    )


def _empty_counts() -> dict[str, int]:
    return {
        "skipped_filtered": 0,
        "skipped_existing": 0,
        "skipped_noncanonical": 0,
        "skipped_unsupported_artifacts": 0,
        "transferred": 0,
        "dry_run": 0,
    }


def _migrate_raw_copy_repository(
    *,
    repository: RepositoryDescriptor,
    canonical_package_type: str,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    for entry in entries:
        target_path = resolve_target_path(canonical_package_type, entry.path)
        if target_path is None:
            counts["skipped_filtered"] += 1
            continue
        if retry_operation(
            f"check target artifact {repository.key}/{target_path}",
            lambda target_path=target_path: pkgly.artifact_exists(
                target_storage_name, repository.key, target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue

        def copy_entry() -> None:
            with closing(artifactory.open_file(repository.key, entry.path)) as stream:
                pkgly.upload_file(
                    target_storage_name,
                    repository.key,
                    target_path,
                    stream,
                    entry.size,
                )

        retry_operation(
            f"copy artifact {repository.key}/{entry.path}",
            copy_entry,
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_npm_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    tarball_entries = [entry for entry in entries if entry.path.endswith(".tgz")]
    counts["skipped_unsupported_artifacts"] += len(entries) - len(tarball_entries)

    parsed_candidates = []
    for entry in tarball_entries:
        package = retry_operation(
            f"read npm package {repository.key}/{entry.path}",
            lambda entry=entry: parse_npm_package_bytes(
                _read_artifactory_file_bytes(artifactory, repository.key, entry.path),
                pkgly_base_url=pkgly.base_url,
                storage_name=target_storage_name,
                repository_name=repository.key,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        parsed_candidates.append((package.package_name, package.version, entry, package))
    parsed_candidates.sort(key=lambda item: (item[0], _version_sort_key(item[1])))

    for _, _, entry, package in parsed_candidates:
        if retry_operation(
            f"check npm tarball {repository.key}/{package.tarball_path}",
            lambda package=package: pkgly.artifact_exists(
                target_storage_name, repository.key, package.tarball_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        retry_operation(
            f"publish npm package {repository.key}/{entry.path}",
            lambda package=package: pkgly.publish_npm_package(
                target_storage_name,
                repository.key,
                package.package_name,
                package.version,
                package.tarball_path,
                package.publish_payload,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_nuget_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    for entry in entries:
        if entry.path.endswith(".snupkg") or entry.path.endswith(".symbols.nupkg"):
            counts["skipped_unsupported_artifacts"] += 1
            continue
        if not entry.path.endswith(".nupkg"):
            counts["skipped_unsupported_artifacts"] += 1
            continue
        package_bytes = retry_operation(
            f"read NuGet package {repository.key}/{entry.path}",
            lambda entry=entry: _read_artifactory_file_bytes(
                artifactory, repository.key, entry.path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        parsed = parse_nuget_package_bytes(entry.path.rsplit("/", 1)[-1], package_bytes)
        if retry_operation(
            f"check NuGet package {repository.key}/{parsed.target_path}",
            lambda parsed=parsed: pkgly.artifact_exists(
                target_storage_name, repository.key, parsed.target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        retry_operation(
            f"publish NuGet package {repository.key}/{entry.path}",
            lambda parsed=parsed, package_bytes=package_bytes, entry=entry: pkgly.publish_nuget_package(
                target_storage_name,
                repository.key,
                parsed.package_id,
                parsed.version,
                entry.path.rsplit("/", 1)[-1],
                package_bytes,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _ruby_target_path(path: str) -> str | None:
    filename = normalize_path(path).rsplit("/", 1)[-1]
    if filename.endswith(".gem"):
        return f"gems/{filename}"
    return None


def _migrate_ruby_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    for entry in entries:
        target_path = _ruby_target_path(entry.path)
        if target_path is None:
            counts["skipped_unsupported_artifacts"] += 1
            continue
        if retry_operation(
            f"check Ruby gem {repository.key}/{target_path}",
            lambda target_path=target_path: pkgly.artifact_exists(
                target_storage_name, repository.key, target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        gem_bytes = retry_operation(
            f"read Ruby gem {repository.key}/{entry.path}",
            lambda entry=entry: _read_artifactory_file_bytes(
                artifactory, repository.key, entry.path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        retry_operation(
            f"publish Ruby gem {repository.key}/{entry.path}",
            lambda gem_bytes=gem_bytes, entry=entry: pkgly.publish_ruby_gem(
                target_storage_name,
                repository.key,
                entry.path.rsplit("/", 1)[-1],
                gem_bytes,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_cargo_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    for entry in entries:
        if not entry.path.endswith(".crate"):
            counts["skipped_unsupported_artifacts"] += 1
            continue
        crate_bytes = retry_operation(
            f"read Cargo crate {repository.key}/{entry.path}",
            lambda entry=entry: _read_artifactory_file_bytes(
                artifactory, repository.key, entry.path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        parsed = parse_cargo_package_bytes(entry.path.rsplit("/", 1)[-1], crate_bytes)
        if retry_operation(
            f"check Cargo crate {repository.key}/{parsed.target_path}",
            lambda parsed=parsed: pkgly.artifact_exists(
                target_storage_name, repository.key, parsed.target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        retry_operation(
            f"publish Cargo crate {repository.key}/{entry.path}",
            lambda parsed=parsed: pkgly.publish_cargo_package(
                target_storage_name,
                repository.key,
                parsed.crate_name,
                parsed.version,
                parsed.payload_bytes,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_go_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> dict[str, int]:
    counts = _empty_counts()
    groups, skipped_noncanonical = group_go_artifacts(entries)
    counts["skipped_noncanonical"] += skipped_noncanonical
    for (module_name, version), group in groups.items():
        if group.zip_entry is None:
            counts["skipped_noncanonical"] += 1
            continue
        target_path = f"{module_name}/@v/{version}.zip"
        if retry_operation(
            f"check Go module {repository.key}/{target_path}",
            lambda target_path=target_path: pkgly.artifact_exists(
                target_storage_name, repository.key, target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        module_zip = retry_operation(
            f"read Go zip {repository.key}/{group.zip_entry.path}",
            lambda group=group: _read_artifactory_file_bytes(
                artifactory, repository.key, group.zip_entry.path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        go_mod = None
        if group.mod_entry is not None:
            go_mod = retry_operation(
                f"read Go mod {repository.key}/{group.mod_entry.path}",
                lambda group=group: _read_artifactory_file_bytes(
                    artifactory, repository.key, group.mod_entry.path
                ),
                retries=retries,
                backoff_seconds=retry_backoff_seconds,
            )
        info_json = None
        if group.info_entry is not None:
            info_json = retry_operation(
                f"read Go info {repository.key}/{group.info_entry.path}",
                lambda group=group: _read_artifactory_file_bytes(
                    artifactory, repository.key, group.info_entry.path
                ),
                retries=retries,
                backoff_seconds=retry_backoff_seconds,
            )
        retry_operation(
            f"publish Go module {repository.key}/{module_name}@{version}",
            lambda module_zip=module_zip, go_mod=go_mod, info_json=info_json: pkgly.upload_go_module(
                target_storage_name,
                repository.key,
                module_name,
                version,
                module_zip,
                go_mod,
                info_json,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_deb_repository(
    *,
    repository: RepositoryDescriptor,
    entries: list[ArtifactEntry],
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
    package_options: dict | None,
) -> dict[str, int]:
    counts = _empty_counts()
    resolved = _resolve_package_options(package_options)
    for entry in entries:
        if not entry.path.endswith(".deb"):
            counts["skipped_unsupported_artifacts"] += 1
            continue
        package_bytes = retry_operation(
            f"read deb package {repository.key}/{entry.path}",
            lambda entry=entry: _read_artifactory_file_bytes(
                artifactory, repository.key, entry.path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        filename = entry.path.rsplit("/", 1)[-1]
        parsed = parse_deb_package_bytes(
            filename,
            package_bytes,
            component=resolved["deb_component"],
        )
        if retry_operation(
            f"check deb package {repository.key}/{parsed.target_path}",
            lambda parsed=parsed: pkgly.artifact_exists(
                target_storage_name, repository.key, parsed.target_path
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        ):
            counts["skipped_existing"] += 1
            continue
        if dry_run:
            counts["dry_run"] += 1
            continue
        retry_operation(
            f"publish deb package {repository.key}/{entry.path}",
            lambda filename=filename, package_bytes=package_bytes, resolved=resolved: pkgly.upload_deb_package(
                target_storage_name,
                repository.key,
                resolved["deb_distribution"],
                resolved["deb_component"],
                filename,
                package_bytes,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        counts["transferred"] += 1
    return counts


def _migrate_docker_manifest_reference(
    *,
    repository: RepositoryDescriptor,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    image_name: str,
    reference: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
    counts: dict[str, int],
    visited: set[tuple[str, str]],
) -> None:
    key = (image_name, reference)
    if key in visited:
        return
    visited.add(key)

    if retry_operation(
        f"check Docker manifest {repository.key}/{image_name}:{reference}",
        lambda: pkgly.docker_manifest_exists(
            target_storage_name, repository.key, image_name, reference
        ),
        retries=retries,
        backoff_seconds=retry_backoff_seconds,
    ):
        counts["skipped_existing"] += 1
        return

    manifest_bytes, media_type = retry_operation(
        f"read Docker manifest {repository.key}/{image_name}:{reference}",
        lambda: artifactory.get_docker_manifest(repository.key, image_name, reference),
        retries=retries,
        backoff_seconds=retry_backoff_seconds,
    )
    if media_type in DOCKER_SCHEMA1_MEDIA_TYPES:
        counts["skipped_unsupported_artifacts"] += 1
        return
    manifest = _safe_json_loads(manifest_bytes)
    for child_digest in _extract_docker_child_manifests(manifest):
        _migrate_docker_manifest_reference(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name=target_storage_name,
            image_name=image_name,
            reference=child_digest,
            dry_run=dry_run,
            retries=retries,
            retry_backoff_seconds=retry_backoff_seconds,
            counts=counts,
            visited=visited,
        )
    for digest in _extract_docker_blob_digests(manifest):
        exists = retry_operation(
            f"check Docker blob {repository.key}/{image_name}@{digest}",
            lambda digest=digest: pkgly.docker_blob_exists(
                target_storage_name, repository.key, image_name, digest
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        if exists:
            continue
        if dry_run:
            continue
        blob_bytes = retry_operation(
            f"read Docker blob {repository.key}/{image_name}@{digest}",
            lambda digest=digest: _read_artifactory_docker_blob_bytes(
                artifactory, repository.key, image_name, digest
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        retry_operation(
            f"upload Docker blob {repository.key}/{image_name}@{digest}",
            lambda digest=digest, blob_bytes=blob_bytes: pkgly.upload_docker_blob(
                target_storage_name, repository.key, image_name, digest, blob_bytes
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
    if dry_run:
        counts["dry_run"] += 1
        return
    retry_operation(
        f"upload Docker manifest {repository.key}/{image_name}:{reference}",
        lambda: pkgly.upload_docker_manifest(
            target_storage_name,
            repository.key,
            image_name,
            reference,
            media_type,
            manifest_bytes,
        ),
        retries=retries,
        backoff_seconds=retry_backoff_seconds,
    )
    counts["transferred"] += 1


def _migrate_docker_repository(
    *,
    repository: RepositoryDescriptor,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    dry_run: bool,
    retries: int,
    retry_backoff_seconds: float,
) -> tuple[int, dict[str, int]]:
    counts = _empty_counts()
    discovered = 0
    images = retry_operation(
        f"list Docker catalog for {repository.key}",
        lambda: artifactory.list_docker_images(repository.key),
        retries=retries,
        backoff_seconds=retry_backoff_seconds,
    )
    visited: set[tuple[str, str]] = set()
    for image_name in images:
        tags = retry_operation(
            f"list Docker tags for {repository.key}/{image_name}",
            lambda image_name=image_name: artifactory.list_docker_tags(repository.key, image_name),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )
        discovered += len(tags)
        for tag in tags:
            _migrate_docker_manifest_reference(
                repository=repository,
                artifactory=artifactory,
                pkgly=pkgly,
                target_storage_name=target_storage_name,
                image_name=image_name,
                reference=tag,
                dry_run=dry_run,
                retries=retries,
                retry_backoff_seconds=retry_backoff_seconds,
                counts=counts,
                visited=visited,
            )
    return discovered, counts


def migrate_repository(
    *,
    repository: RepositoryDescriptor,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    create_targets: bool,
    path_prefix: str,
    dry_run: bool,
    retries: int = DEFAULT_RETRIES,
    retry_backoff_seconds: float = DEFAULT_RETRY_BACKOFF_SECONDS,
    package_options: dict | None = None,
) -> RepositoryMigrationResult:
    canonical_package_type = map_artifactory_package_type(repository.package_type)
    if repository.repo_type not in SUPPORTED_REPO_TYPES:
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=f"unsupported repository class: {repository.repo_type}",
        )
    if canonical_package_type not in SUPPORTED_PACKAGE_TYPES:
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=f"unsupported package type: {repository.package_type}",
        )

    try:
        retry_operation(
            f"prepare target repository {repository.key}",
            lambda: prepare_target_repository(
                pkgly=pkgly,
                storage_name=target_storage_name,
                repository=repository,
                create_targets=create_targets,
                package_options=package_options,
            ),
            retries=retries,
            backoff_seconds=retry_backoff_seconds,
        )

        if canonical_package_type == "docker":
            discovered, counts = _migrate_docker_repository(
                repository=repository,
                artifactory=artifactory,
                pkgly=pkgly,
                target_storage_name=target_storage_name,
                dry_run=dry_run,
                retries=retries,
                retry_backoff_seconds=retry_backoff_seconds,
            )
        else:
            entries = retry_operation(
                f"list files for {repository.key}",
                lambda: artifactory.list_files(repository.key, path_prefix),
                retries=retries,
                backoff_seconds=retry_backoff_seconds,
            )
            discovered = len(entries)
            if canonical_package_type in RAW_COPY_PACKAGE_TYPES:
                counts = _migrate_raw_copy_repository(
                    repository=repository,
                    canonical_package_type=canonical_package_type,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "npm":
                counts = _migrate_npm_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "nuget":
                counts = _migrate_nuget_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "gems":
                counts = _migrate_ruby_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "cargo":
                counts = _migrate_cargo_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "go":
                counts = _migrate_go_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
            elif canonical_package_type == "deb":
                counts = _migrate_deb_repository(
                    repository=repository,
                    entries=entries,
                    artifactory=artifactory,
                    pkgly=pkgly,
                    target_storage_name=target_storage_name,
                    dry_run=dry_run,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                    package_options=package_options,
                )
            else:
                raise RuntimeError(f"unsupported package type: {repository.package_type}")
    except (RuntimeError, HttpStatusError, error.HTTPError) as exc:
        if isinstance(exc, error.HTTPError):
            body = exc.read().decode("utf-8", errors="replace")
            message = f"HTTP {exc.code}: {body}"
        else:
            message = str(exc)
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=message,
        )

    return RepositoryMigrationResult(
        repository_key=repository.key,
        package_type=repository.package_type,
        repo_type=repository.repo_type,
        status="success",
        discovered=discovered,
        skipped_filtered=counts["skipped_filtered"],
        skipped_existing=counts["skipped_existing"],
        skipped_noncanonical=counts["skipped_noncanonical"],
        skipped_unsupported_artifacts=counts["skipped_unsupported_artifacts"],
        transferred=counts["transferred"],
        dry_run=counts["dry_run"],
    )


def migrate_repositories(
    *,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    requested_names: list[str],
    all_repositories: bool,
    target_storage_name: str,
    create_targets: bool,
    path_prefix: str,
    dry_run: bool,
    parallelism: int,
    retries: int = DEFAULT_RETRIES,
    retry_backoff_seconds: float = DEFAULT_RETRY_BACKOFF_SECONDS,
    executor_class=concurrent.futures.ThreadPoolExecutor,
    package_options: dict | None = None,
) -> list[RepositoryMigrationResult]:
    available = retry_operation(
        "list Artifactory repositories",
        artifactory.list_repositories,
        retries=retries,
        backoff_seconds=retry_backoff_seconds,
    )
    selected, missing = select_repositories(available, requested_names, all_repositories)

    results = []
    with executor_class(max_workers=parallelism) as executor:
        futures = [
            executor.submit(
                migrate_repository,
                repository=repository,
                artifactory=artifactory,
                pkgly=pkgly,
                target_storage_name=target_storage_name,
                create_targets=create_targets,
                path_prefix=path_prefix,
                dry_run=dry_run,
                retries=retries,
                retry_backoff_seconds=retry_backoff_seconds,
                package_options=package_options,
            )
            for repository in selected
        ]
        for future in futures:
            results.append(future.result())

    for name in missing:
        results.append(
            RepositoryMigrationResult(
                repository_key=name,
                package_type="",
                repo_type="",
                status="failed",
                error="repository not found in Artifactory",
            )
        )
    return results


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Migrate supported Artifactory repositories into Pkgly."
    )
    parser.add_argument("--artifactory-url", required=True)
    parser.add_argument("--artifactory-token")
    parser.add_argument("--artifactory-user")
    parser.add_argument("--artifactory-password")

    parser.add_argument("--pkgly-url", required=True)
    parser.add_argument("--pkgly-storage", required=True)
    parser.add_argument("--pkgly-token")
    parser.add_argument("--pkgly-user")
    parser.add_argument("--pkgly-password")

    parser.add_argument("--repo", action="append", default=[])
    parser.add_argument("--all-repos", action="store_true")
    parser.add_argument("--path-prefix", default="")
    parser.add_argument("--parallelism", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument("--retries", type=int, default=DEFAULT_RETRIES)
    parser.add_argument(
        "--retry-backoff-seconds",
        type=float,
        default=DEFAULT_RETRY_BACKOFF_SECONDS,
    )
    parser.add_argument("--create-targets", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--deb-distribution", default="stable")
    parser.add_argument("--deb-component", default="main")
    parser.add_argument("--deb-architectures", default="amd64,all")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    if not args.all_repos and not args.repo:
        parser.error("pass --repo at least once or use --all-repos")
    if args.parallelism < 1:
        parser.error("--parallelism must be at least 1")
    if args.retries < 0:
        parser.error("--retries must be at least 0")
    if args.retry_backoff_seconds < 0:
        parser.error("--retry-backoff-seconds must be at least 0")

    deb_architectures = [item.strip() for item in args.deb_architectures.split(",") if item.strip()]
    if not deb_architectures:
        parser.error("--deb-architectures must include at least one architecture")

    try:
        artifactory_auth = build_auth_header(
            resolve_argument(args.artifactory_token, "ARTIFACTORY_TOKEN"),
            resolve_argument(args.artifactory_user, "ARTIFACTORY_USER"),
            resolve_argument(args.artifactory_password, "ARTIFACTORY_PASSWORD"),
        )
        pkgly_auth = build_auth_header(
            resolve_argument(args.pkgly_token, "PKGLY_TOKEN"),
            resolve_argument(args.pkgly_user, "PKGLY_USER"),
            resolve_argument(args.pkgly_password, "PKGLY_PASSWORD"),
        )
    except ValueError as exc:
        parser.error(str(exc))
    package_options = {
        "deb_distribution": args.deb_distribution,
        "deb_component": args.deb_component,
        "deb_architectures": deb_architectures,
    }

    artifactory = ArtifactoryClient(
        base_url=args.artifactory_url,
        auth_header=artifactory_auth,
        timeout=args.timeout,
    )
    pkgly = PkglyClient(
        base_url=args.pkgly_url,
        auth_header=pkgly_auth,
        timeout=args.timeout,
    )
    results = migrate_repositories(
        artifactory=artifactory,
        pkgly=pkgly,
        requested_names=args.repo,
        all_repositories=args.all_repos,
        target_storage_name=args.pkgly_storage,
        create_targets=args.create_targets,
        path_prefix=args.path_prefix,
        dry_run=args.dry_run,
        parallelism=args.parallelism,
        retries=args.retries,
        retry_backoff_seconds=args.retry_backoff_seconds,
        package_options=package_options,
    )
    for result in results:
        print(json.dumps(result.__dict__, sort_keys=True))
    return 0 if all(result.status == "success" for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
