#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import concurrent.futures
import http.client
import json
import sys
from contextlib import closing
from dataclasses import dataclass
from typing import BinaryIO
from urllib import error, parse, request


CHECKSUM_SUFFIXES = (".sha1", ".sha256", ".sha512", ".md5")
DEFAULT_TIMEOUT_SECONDS = 60
USER_AGENT = "pkgly-artifactory-to-pkgly/0.2"
SUPPORTED_PACKAGE_TYPES = {"maven"}
SUPPORTED_REPO_TYPES = {"local"}


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
    transferred: int = 0
    dry_run: int = 0
    error: str = ""


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


def should_copy_path(package_type: str, path: str) -> bool:
    if package_type == "maven":
        return not path.lower().endswith(CHECKSUM_SUFFIXES)
    return True


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


def _json_request(
    url: str,
    *,
    method: str,
    auth_header: str | None,
    timeout: int,
    payload: dict | None = None,
) -> dict | list:
    headers = {"Accept": "application/json", "User-Agent": USER_AGENT}
    data = None
    if auth_header:
        headers["Authorization"] = auth_header
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
    req = request.Request(url, data=data, headers=headers, method=method)
    with request.urlopen(req, timeout=timeout) as response:
        body = response.read()
    if not body:
        return {}
    return json.loads(body.decode("utf-8"))


def _stream_put(
    url: str,
    *,
    auth_header: str | None,
    timeout: int,
    stream: BinaryIO,
    size: int,
) -> None:
    parsed = parse.urlsplit(url)
    connection_class = (
        http.client.HTTPSConnection if parsed.scheme == "https" else http.client.HTTPConnection
    )
    connection = connection_class(parsed.hostname, parsed.port, timeout=timeout)
    request_path = parsed.path or "/"
    if parsed.query:
        request_path = f"{request_path}?{parsed.query}"

    connection.putrequest("PUT", request_path)
    connection.putheader("User-Agent", USER_AGENT)
    connection.putheader("Content-Length", str(size))
    if auth_header:
        connection.putheader("Authorization", auth_header)
    connection.endheaders()

    while True:
        chunk = stream.read(64 * 1024)
        if not chunk:
            break
        connection.send(chunk)

    response = connection.getresponse()
    body = response.read().decode("utf-8", errors="replace")
    connection.close()
    if response.status < 200 or response.status >= 300:
        raise RuntimeError(f"Pkgly upload failed with HTTP {response.status}: {body}")


def _head_request(url: str, *, auth_header: str | None, timeout: int) -> int:
    headers = {"User-Agent": USER_AGENT}
    if auth_header:
        headers["Authorization"] = auth_header
    req = request.Request(url, headers=headers, method="HEAD")
    try:
        with request.urlopen(req, timeout=timeout) as response:
            return response.status
    except error.HTTPError as exc:
        return exc.code


class ArtifactoryClient:
    def __init__(self, *, base_url: str, auth_header: str | None, timeout: int) -> None:
        self.base_url = normalize_base_url(base_url)
        self.auth_header = auth_header
        self.timeout = timeout

    def list_repositories(self) -> list[RepositoryDescriptor]:
        url = f"{self.base_url}/artifactory/api/repositories"
        payload = _json_request(
            url,
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
        url = f"{base}?deep=1&listFolders=0"
        payload = _json_request(
            url,
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


class PkglyClient:
    def __init__(self, *, base_url: str, auth_header: str | None, timeout: int) -> None:
        self.base_url = normalize_base_url(base_url)
        self.auth_header = auth_header
        self.timeout = timeout

    def repository_exists(self, storage_name: str, repository_name: str) -> bool:
        url = (
            f"{self.base_url}/api/repository/find-id/"
            f"{parse.quote(storage_name)}/{parse.quote(repository_name)}"
        )
        try:
            _json_request(
                url,
                method="GET",
                auth_header=self.auth_header,
                timeout=self.timeout,
            )
            return True
        except error.HTTPError as exc:
            if exc.code == 404:
                return False
            raise RuntimeError(f"Pkgly repository lookup failed with HTTP {exc.code}") from exc

    def create_repository(self, storage_id: str, repository_name: str, package_type: str) -> None:
        if package_type != "maven":
            raise RuntimeError(f"unsupported package type for target creation: {package_type}")
        payload = {
            "name": repository_name,
            "storage": storage_id,
            "configs": {
                "maven": {
                    "type": "Hosted",
                }
            },
        }
        url = f"{self.base_url}/api/repository/new/maven"
        try:
            _json_request(
                url,
                method="POST",
                auth_header=self.auth_header,
                timeout=self.timeout,
                payload=payload,
            )
        except error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(
                f"Pkgly repository creation failed with HTTP {exc.code}: {body}"
            ) from exc

    def artifact_exists(self, storage_name: str, repository_name: str, relative_path: str) -> bool:
        url = (
            f"{self.base_url}/repositories/"
            f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(relative_path)}"
        )
        status = _head_request(url, auth_header=self.auth_header, timeout=self.timeout)
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
        url = (
            f"{self.base_url}/repositories/"
            f"{parse.quote(storage_name)}/{parse.quote(repository_name)}/{quote_path(relative_path)}"
        )
        _stream_put(
            url,
            auth_header=self.auth_header,
            timeout=self.timeout,
            stream=stream,
            size=size,
        )


def prepare_target_repository(
    *,
    pkgly: PkglyClient,
    storage_name: str,
    storage_id: str | None,
    repository: RepositoryDescriptor,
    create_targets: bool,
) -> None:
    if repository.package_type not in SUPPORTED_PACKAGE_TYPES:
        raise RuntimeError(f"unsupported package type: {repository.package_type}")
    if pkgly.repository_exists(storage_name, repository.key):
        return
    if not create_targets:
        raise RuntimeError(f"target repository {storage_name}/{repository.key} does not exist")
    if not storage_id:
        raise RuntimeError("--pkgly-storage-id is required when --create-targets is used")
    pkgly.create_repository(storage_id, repository.key, repository.package_type)


def migrate_repository(
    *,
    repository: RepositoryDescriptor,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    target_storage_name: str,
    create_targets: bool,
    target_storage_id: str | None,
    path_prefix: str,
    dry_run: bool,
) -> RepositoryMigrationResult:
    if repository.repo_type not in SUPPORTED_REPO_TYPES:
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=f"unsupported repository class: {repository.repo_type}",
        )

    if repository.package_type not in SUPPORTED_PACKAGE_TYPES:
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=f"unsupported package type: {repository.package_type}",
        )

    try:
        prepare_target_repository(
            pkgly=pkgly,
            storage_name=target_storage_name,
            storage_id=target_storage_id,
            repository=repository,
            create_targets=create_targets,
        )

        entries = artifactory.list_files(repository.key, path_prefix)
        skipped_filtered = 0
        skipped_existing = 0
        transferred = 0
        dry_run_count = 0

        for entry in entries:
            if not should_copy_path(repository.package_type, entry.path):
                skipped_filtered += 1
                continue

            if pkgly.artifact_exists(target_storage_name, repository.key, entry.path):
                skipped_existing += 1
                continue

            if dry_run:
                dry_run_count += 1
                continue

            with closing(artifactory.open_file(repository.key, entry.path)) as stream:
                pkgly.upload_file(
                    target_storage_name,
                    repository.key,
                    entry.path,
                    stream,
                    entry.size,
                )
            transferred += 1
    except RuntimeError as exc:
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=str(exc),
        )
    except error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        return RepositoryMigrationResult(
            repository_key=repository.key,
            package_type=repository.package_type,
            repo_type=repository.repo_type,
            status="failed",
            error=f"HTTP {exc.code}: {body}",
        )

    return RepositoryMigrationResult(
        repository_key=repository.key,
        package_type=repository.package_type,
        repo_type=repository.repo_type,
        status="success",
        discovered=len(entries),
        skipped_filtered=skipped_filtered,
        skipped_existing=skipped_existing,
        transferred=transferred,
        dry_run=dry_run_count,
    )


def migrate_repositories(
    *,
    artifactory: ArtifactoryClient,
    pkgly: PkglyClient,
    requested_names: list[str],
    all_repositories: bool,
    target_storage_name: str,
    create_targets: bool,
    target_storage_id: str | None,
    path_prefix: str,
    dry_run: bool,
    parallelism: int,
    executor_class=concurrent.futures.ThreadPoolExecutor,
) -> list[RepositoryMigrationResult]:
    available = artifactory.list_repositories()
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
                target_storage_id=target_storage_id,
                path_prefix=path_prefix,
                dry_run=dry_run,
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
    parser.add_argument("--pkgly-storage-id")

    parser.add_argument("--repo", action="append", default=[])
    parser.add_argument("--all-repos", action="store_true")
    parser.add_argument("--path-prefix", default="")
    parser.add_argument("--parallelism", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument("--create-targets", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    if not args.all_repos and not args.repo:
        parser.error("pass --repo at least once or use --all-repos")
    if args.parallelism < 1:
        parser.error("--parallelism must be at least 1")

    try:
        artifactory_auth = build_auth_header(
            args.artifactory_token,
            args.artifactory_user,
            args.artifactory_password,
        )
        pkgly_auth = build_auth_header(
            args.pkgly_token,
            args.pkgly_user,
            args.pkgly_password,
        )
    except ValueError as exc:
        parser.error(str(exc))

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
        target_storage_id=args.pkgly_storage_id,
        path_prefix=args.path_prefix,
        dry_run=args.dry_run,
        parallelism=args.parallelism,
    )

    for result in results:
        print(json.dumps(result.__dict__, sort_keys=True))

    return 0 if all(result.status == "success" for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
