import io
import json
import sys
import tarfile
import textwrap
import unittest
import zipfile
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import artifactory_to_pkgly as migrator  # noqa: E402


def _build_tar_gz(files):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(content)
            archive.addfile(info, io.BytesIO(content))
    return buffer.getvalue()


def build_npm_tgz(package_json, extra_files=None):
    files = {
        "package/package.json": json.dumps(package_json, sort_keys=True).encode("utf-8"),
    }
    for name, content in (extra_files or {}).items():
        files[f"package/{name}"] = content
    return _build_tar_gz(files)


def build_cargo_crate(crate_name, version):
    cargo_toml = textwrap.dedent(
        f"""
        [package]
        name = "{crate_name}"
        version = "{version}"
        description = "Example crate"
        license = "MIT"
        authors = ["Pkgly"]
        repository = "https://example.test/{crate_name}"

        [dependencies]
        serde = "1"
        """
    ).strip()
    root = f"{crate_name}-{version}"
    return _build_tar_gz(
        {
            f"{root}/Cargo.toml": cargo_toml.encode("utf-8"),
            f"{root}/src/lib.rs": b"pub fn demo() {}\n",
        }
    )


def build_nupkg(package_id, version):
    nuspec = textwrap.dedent(
        f"""\
        <?xml version="1.0" encoding="utf-8"?>
        <package>
          <metadata>
            <id>{package_id}</id>
            <version>{version}</version>
            <authors>Pkgly</authors>
            <description>Example package</description>
          </metadata>
        </package>
        """
    ).encode("utf-8")
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(f"{package_id}.nuspec", nuspec)
        archive.writestr("lib/net8.0/example.dll", b"dll")
    return buffer.getvalue()


def build_go_module_zip(module_path, version):
    escaped = f"{module_path}@{version}"
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        archive.writestr(f"{escaped}/go.mod", f"module {module_path}\n".encode("utf-8"))
        archive.writestr(f"{escaped}/main.go", b"package main\n")
    return buffer.getvalue()


def build_deb(package_name, version, architecture):
    control_text = textwrap.dedent(
        f"""\
        Package: {package_name}
        Version: {version}
        Architecture: {architecture}
        Description: Example package
        """
    ).encode("utf-8")

    control_tar = io.BytesIO()
    with tarfile.open(fileobj=control_tar, mode="w:gz") as archive:
        info = tarfile.TarInfo("./control")
        info.size = len(control_text)
        archive.addfile(info, io.BytesIO(control_text))

    data_tar = io.BytesIO()
    with tarfile.open(fileobj=data_tar, mode="w:gz") as archive:
        info = tarfile.TarInfo("./usr/share/doc/example.txt")
        info.size = 0
        archive.addfile(info, io.BytesIO(b""))

    return build_ar_archive(
        [
            ("debian-binary", b"2.0\n"),
            ("control.tar.gz", control_tar.getvalue()),
            ("data.tar.gz", data_tar.getvalue()),
        ]
    )


def build_ar_archive(entries):
    buffer = io.BytesIO()
    buffer.write(b"!<arch>\n")
    for name, content in entries:
        name_bytes = name.encode("utf-8")
        if len(name_bytes) > 16:
            raise ValueError("ar name too long for test helper")
        header = (
            name_bytes.ljust(16, b" ")
            + b"0".ljust(12, b" ")
            + b"0".ljust(6, b" ")
            + b"0".ljust(6, b" ")
            + b"100644".ljust(8, b" ")
            + str(len(content)).encode("ascii").ljust(10, b" ")
            + b"`\n"
        )
        buffer.write(header)
        buffer.write(content)
        if len(content) % 2 == 1:
            buffer.write(b"\n")
    return buffer.getvalue()


class FakeArtifactoryClient:
    def __init__(
        self,
        repositories,
        entries_by_repo=None,
        contents=None,
        docker_catalog=None,
        docker_tags=None,
        docker_manifests=None,
        docker_blobs=None,
    ):
        self._repositories = list(repositories)
        self._entries_by_repo = entries_by_repo or {}
        self._contents = contents or {}
        self._docker_catalog = docker_catalog or {}
        self._docker_tags = docker_tags or {}
        self._docker_manifests = docker_manifests or {}
        self._docker_blobs = docker_blobs or {}
        self.list_repositories_calls = 0
        self.list_files_calls = []
        self.open_file_calls = []
        self.docker_catalog_calls = []
        self.docker_tags_calls = []
        self.docker_manifest_calls = []
        self.docker_blob_calls = []

    def list_repositories(self):
        self.list_repositories_calls += 1
        return list(self._repositories)

    def list_files(self, repository_key, path_prefix=""):
        self.list_files_calls.append((repository_key, path_prefix))
        return list(self._entries_by_repo.get(repository_key, []))

    def open_file(self, repository_key, path):
        self.open_file_calls.append((repository_key, path))
        return io.BytesIO(self._contents[(repository_key, path)])

    def list_docker_images(self, repository_key):
        self.docker_catalog_calls.append(repository_key)
        return list(self._docker_catalog.get(repository_key, []))

    def list_docker_tags(self, repository_key, image_name):
        self.docker_tags_calls.append((repository_key, image_name))
        return list(self._docker_tags.get((repository_key, image_name), []))

    def get_docker_manifest(self, repository_key, image_name, reference):
        self.docker_manifest_calls.append((repository_key, image_name, reference))
        return self._docker_manifests[(repository_key, image_name, reference)]

    def open_docker_blob(self, repository_key, image_name, digest):
        self.docker_blob_calls.append((repository_key, image_name, digest))
        return io.BytesIO(self._docker_blobs[(repository_key, image_name, digest)])


class FakePkglyClient:
    def __init__(
        self,
        existing_repositories=None,
        existing_artifacts=None,
        existing_docker_blobs=None,
        existing_docker_manifests=None,
    ):
        self.base_url = "https://pkgly.example.test"
        self._existing_repositories = set(existing_repositories or [])
        self._existing_artifacts = set(existing_artifacts or [])
        self._existing_docker_blobs = set(existing_docker_blobs or [])
        self._existing_docker_manifests = set(existing_docker_manifests or [])
        self.repository_exists_calls = []
        self.create_calls = []
        self.artifact_exists_calls = []
        self.upload_calls = []
        self.npm_publish_calls = []
        self.nuget_publish_calls = []
        self.ruby_publish_calls = []
        self.cargo_publish_calls = []
        self.go_upload_calls = []
        self.deb_upload_calls = []
        self.docker_blob_exists_calls = []
        self.docker_blob_upload_calls = []
        self.docker_manifest_exists_calls = []
        self.docker_manifest_upload_calls = []

    def repository_exists(self, storage_name, repository_name):
        self.repository_exists_calls.append((storage_name, repository_name))
        return (storage_name, repository_name) in self._existing_repositories

    def create_repository(self, storage_name, repository_name, package_type, package_options=None):
        self.create_calls.append((storage_name, repository_name, package_type, package_options))
        self._existing_repositories.add(("target-storage", repository_name))

    def artifact_exists(self, storage_name, repository_name, relative_path):
        self.artifact_exists_calls.append((storage_name, repository_name, relative_path))
        return (storage_name, repository_name, relative_path) in self._existing_artifacts

    def upload_file(self, storage_name, repository_name, relative_path, stream, size):
        self.upload_calls.append(
            (storage_name, repository_name, relative_path, size, stream.read())
        )

    def publish_npm_package(self, storage_name, repository_name, package_name, version, tarball_path, payload):
        self.npm_publish_calls.append(
            (storage_name, repository_name, package_name, version, tarball_path, payload)
        )

    def publish_nuget_package(self, storage_name, repository_name, package_id, version, filename, package_bytes):
        self.nuget_publish_calls.append(
            (storage_name, repository_name, package_id, version, filename, package_bytes)
        )

    def publish_ruby_gem(self, storage_name, repository_name, filename, gem_bytes):
        self.ruby_publish_calls.append(
            (storage_name, repository_name, filename, gem_bytes)
        )

    def publish_cargo_package(self, storage_name, repository_name, crate_name, version, payload_bytes):
        self.cargo_publish_calls.append(
            (storage_name, repository_name, crate_name, version, payload_bytes)
        )

    def upload_go_module(
        self,
        storage_name,
        repository_name,
        module_name,
        version,
        module_zip,
        go_mod,
        info_json,
    ):
        self.go_upload_calls.append(
            (storage_name, repository_name, module_name, version, module_zip, go_mod, info_json)
        )

    def upload_deb_package(
        self,
        storage_name,
        repository_name,
        distribution,
        component,
        filename,
        package_bytes,
    ):
        self.deb_upload_calls.append(
            (storage_name, repository_name, distribution, component, filename, package_bytes)
        )

    def docker_blob_exists(self, storage_name, repository_name, image_name, digest):
        self.docker_blob_exists_calls.append((storage_name, repository_name, image_name, digest))
        return (storage_name, repository_name, image_name, digest) in self._existing_docker_blobs

    def upload_docker_blob(self, storage_name, repository_name, image_name, digest, blob_bytes):
        self.docker_blob_upload_calls.append(
            (storage_name, repository_name, image_name, digest, blob_bytes)
        )

    def docker_manifest_exists(self, storage_name, repository_name, image_name, reference):
        self.docker_manifest_exists_calls.append(
            (storage_name, repository_name, image_name, reference)
        )
        return (
            storage_name,
            repository_name,
            image_name,
            reference,
        ) in self._existing_docker_manifests

    def upload_docker_manifest(
        self,
        storage_name,
        repository_name,
        image_name,
        reference,
        media_type,
        manifest_bytes,
    ):
        self.docker_manifest_upload_calls.append(
            (storage_name, repository_name, image_name, reference, media_type, manifest_bytes)
        )


class FakeFuture:
    def __init__(self, fn, args, kwargs):
        self._fn = fn
        self._args = args
        self._kwargs = kwargs

    def result(self):
        return self._fn(*self._args, **self._kwargs)


class FakeExecutor:
    last_max_workers = None

    def __init__(self, max_workers):
        type(self).last_max_workers = max_workers

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def submit(self, fn, *args, **kwargs):
        return FakeFuture(fn, args, kwargs)


class SelectionTests(unittest.TestCase):
    def test_select_repositories_filters_requested_names(self):
        available = [
            migrator.RepositoryDescriptor(
                key="maven-local",
                package_type="maven",
                repo_type="local",
            ),
            migrator.RepositoryDescriptor(
                key="npm-local",
                package_type="npm",
                repo_type="local",
            ),
        ]

        selected, missing = migrator.select_repositories(
            available,
            requested_names=["npm-local", "missing"],
            all_repositories=False,
        )

        self.assertEqual([repo.key for repo in selected], ["npm-local"])
        self.assertEqual(missing, ["missing"])

    def test_parse_repositories_response_extracts_type_information(self):
        payload = [
            {"key": "libs-release-local", "packageType": "maven", "repoType": "local"},
            {"key": "docker-prod", "packageType": "docker", "repoType": "remote"},
        ]

        result = migrator.parse_repositories_response(payload)

        self.assertEqual(
            result,
            [
                migrator.RepositoryDescriptor(
                    key="libs-release-local",
                    package_type="maven",
                    repo_type="local",
                ),
                migrator.RepositoryDescriptor(
                    key="docker-prod",
                    package_type="docker",
                    repo_type="remote",
                ),
            ],
        )


class HelperTests(unittest.TestCase):
    def test_resolve_argument_prefers_explicit_value(self):
        with mock.patch.dict("os.environ", {"PKGLY_TOKEN": "env-token"}, clear=True):
            self.assertEqual(
                migrator.resolve_argument("cli-token", "PKGLY_TOKEN"),
                "cli-token",
            )

    def test_resolve_argument_reads_environment_when_missing(self):
        with mock.patch.dict("os.environ", {"PKGLY_TOKEN": "env-token"}, clear=True):
            self.assertEqual(
                migrator.resolve_argument(None, "PKGLY_TOKEN"),
                "env-token",
            )

    def test_retry_operation_retries_transient_failures(self):
        attempts = []

        def flaky():
            attempts.append("called")
            if len(attempts) < 3:
                raise migrator.error.HTTPError(
                    "https://example.test",
                    503,
                    "service unavailable",
                    hdrs=None,
                    fp=io.BytesIO(b"busy"),
                )
            return "ok"

        sleep_calls = []
        result = migrator.retry_operation(
            "flaky request",
            flaky,
            retries=3,
            backoff_seconds=0.5,
            sleep=sleep_calls.append,
        )

        self.assertEqual(result, "ok")
        self.assertEqual(len(attempts), 3)
        self.assertEqual(sleep_calls, [0.5, 1.0])

    def test_retry_operation_does_not_retry_non_transient_http_error(self):
        attempts = []

        def missing():
            attempts.append("called")
            raise migrator.error.HTTPError(
                "https://example.test",
                404,
                "not found",
                hdrs=None,
                fp=io.BytesIO(b"missing"),
            )

        with self.assertRaises(migrator.error.HTTPError):
            migrator.retry_operation(
                "missing request",
                missing,
                retries=3,
                backoff_seconds=0.5,
                sleep=lambda _: None,
            )

        self.assertEqual(len(attempts), 1)

    def test_map_package_type_treats_maven_aliases_as_maven(self):
        self.assertEqual(migrator.map_artifactory_package_type("gradle"), "maven")
        self.assertEqual(migrator.map_artifactory_package_type("ivy"), "maven")
        self.assertEqual(migrator.map_artifactory_package_type("sbt"), "maven")
        self.assertEqual(migrator.map_artifactory_package_type("gems"), "gems")

    def test_resolve_target_path_converts_pypi_filename(self):
        self.assertEqual(
            migrator.resolve_target_path("pypi", "demo-package-1.2.3.tar.gz"),
            "demo-package/1.2.3/demo-package-1.2.3.tar.gz",
        )

    def test_resolve_target_path_canonicalizes_helm_chart(self):
        self.assertEqual(
            migrator.resolve_target_path("helm", "nested/my-chart-1.0.0.tgz"),
            "charts/my-chart/my-chart-1.0.0.tgz",
        )

    def test_resolve_target_path_canonicalizes_php_dist(self):
        self.assertEqual(
            migrator.resolve_target_path("composer", "acme/demo/1.2.3.zip"),
            "dist/acme/demo/1.2.3.zip",
        )
        self.assertEqual(
            migrator.resolve_target_path(
                "composer",
                "dist/acme/demo/1.2.3/demo-1.2.3.zip",
            ),
            "dist/acme/demo/demo-1.2.3.zip",
        )

    def test_parse_npm_package_builds_publish_request(self):
        tgz = build_npm_tgz(
            {
                "name": "@acme/demo",
                "version": "1.2.3",
                "description": "Scoped package",
                "main": "index.js",
            },
            extra_files={"index.js": b"module.exports = 1;\n"},
        )

        package = migrator.parse_npm_package_bytes(
            tgz,
            pkgly_base_url="https://pkgly.example.test",
            storage_name="main",
            repository_name="npm-local",
        )

        self.assertEqual(package.package_name, "@acme/demo")
        self.assertEqual(package.version, "1.2.3")
        self.assertEqual(package.tarball_path, "@acme/demo/-/demo-1.2.3.tgz")
        self.assertIn("1.2.3", package.publish_payload["versions"])
        self.assertIn("@acme/demo/-/demo-1.2.3.tgz", package.publish_payload["_attachments"])

    def test_parse_nuget_package_extracts_id_and_version(self):
        parsed = migrator.parse_nuget_package_bytes(
            "Acme.Tools.1.2.3.nupkg",
            build_nupkg("Acme.Tools", "1.2.3"),
        )

        self.assertEqual(parsed.package_id, "Acme.Tools")
        self.assertEqual(parsed.version, "1.2.3")
        self.assertEqual(
            parsed.target_path,
            "v3/flatcontainer/acme.tools/1.2.3/acme.tools.1.2.3.nupkg",
        )

    def test_build_cargo_publish_body_extracts_metadata(self):
        payload = migrator.build_cargo_publish_body(build_cargo_crate("demo-crate", "1.2.3"))
        metadata = migrator.parse_cargo_publish_body(payload)["metadata"]

        self.assertEqual(metadata["name"], "demo-crate")
        self.assertEqual(metadata["vers"], "1.2.3")
        self.assertEqual(metadata["deps"][0]["name"], "serde")

    def test_group_go_artifacts_requires_zip_and_canonical_path(self):
        groups, skipped = migrator.group_go_artifacts(
            [
                migrator.ArtifactEntry("github.com/acme/demo/@v/v1.2.3.zip", 5),
                migrator.ArtifactEntry("github.com/acme/demo/@v/v1.2.3.mod", 4),
                migrator.ArtifactEntry("github.com/acme/demo/v1.2.3.info", 4),
            ]
        )

        self.assertEqual(list(groups.keys()), [("github.com/acme/demo", "v1.2.3")])
        self.assertEqual(skipped, 1)

    def test_parse_deb_package_extracts_metadata_and_target_path(self):
        parsed = migrator.parse_deb_package_bytes(
            "hello_1.0.0_amd64.deb",
            build_deb("hello", "1.0.0", "amd64"),
            component="main",
        )

        self.assertEqual(parsed.package_name, "hello")
        self.assertEqual(parsed.version, "1.0.0")
        self.assertEqual(parsed.architecture, "amd64")
        self.assertEqual(parsed.target_path, "pool/main/h/hello/hello_1.0.0_amd64.deb")


class RepositoryMigrationTests(unittest.TestCase):
    def test_migrate_repository_skips_existing_and_filtered_files(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "maven-local": [
                    migrator.ArtifactEntry("com/acme/app/1.0/app-1.0.jar", 3),
                    migrator.ArtifactEntry("com/acme/app/1.0/app-1.0.jar.sha1", 40),
                    migrator.ArtifactEntry("com/acme/app/maven-metadata.xml", 5),
                ]
            },
            contents={
                ("maven-local", "com/acme/app/1.0/app-1.0.jar"): b"jar",
                ("maven-local", "com/acme/app/maven-metadata.xml"): b"<xml>",
            },
        )
        pkgly = FakePkglyClient(
            existing_repositories={("target-storage", "maven-local")},
            existing_artifacts={("target-storage", "maven-local", "com/acme/app/maven-metadata.xml")},
        )
        repository = migrator.RepositoryDescriptor(
            key="maven-local",
            package_type="maven",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.discovered, 3)
        self.assertEqual(result.skipped_filtered, 1)
        self.assertEqual(result.skipped_existing, 1)
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_noncanonical, 0)
        self.assertEqual(result.skipped_unsupported_artifacts, 0)
        self.assertEqual(
            pkgly.upload_calls,
            [
                (
                    "target-storage",
                    "maven-local",
                    "com/acme/app/1.0/app-1.0.jar",
                    3,
                    b"jar",
                )
            ],
        )

    def test_migrate_repository_marks_non_local_repo_failed(self):
        artifactory = FakeArtifactoryClient(repositories=[])
        pkgly = FakePkglyClient()
        repository = migrator.RepositoryDescriptor(
            key="maven-remote",
            package_type="maven",
            repo_type="remote",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "failed")
        self.assertIn("unsupported repository class", result.error)

    def test_migrate_repository_creates_supported_target_when_requested(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={"maven-local": []},
        )
        pkgly = FakePkglyClient()
        repository = migrator.RepositoryDescriptor(
            key="maven-local",
            package_type="maven",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=True,
            path_prefix="",
            dry_run=False,
            retries=0,
            retry_backoff_seconds=0.1,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(
            pkgly.create_calls,
            [
                (
                    "target-storage",
                    "maven-local",
                    "maven",
                    None,
                )
            ],
        )

    def test_migrate_repository_retries_upload_and_maps_pypi_paths(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "pypi-local": [
                    migrator.ArtifactEntry("demo-package-1.2.3.tar.gz", 7),
                ]
            },
            contents={
                ("pypi-local", "demo-package-1.2.3.tar.gz"): b"package",
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "pypi-local")})
        repository = migrator.RepositoryDescriptor(
            key="pypi-local",
            package_type="pypi",
            repo_type="local",
        )
        original_upload = pkgly.upload_file
        attempts = []

        def flaky_upload(storage_name, repository_name, relative_path, stream, size):
            attempts.append(relative_path)
            if len(attempts) == 1:
                raise migrator.HttpStatusError(503, "busy")
            return original_upload(storage_name, repository_name, relative_path, stream, size)

        pkgly.upload_file = flaky_upload

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
            retries=1,
            retry_backoff_seconds=0.1,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(len(attempts), 2)
        self.assertEqual(
            pkgly.upload_calls,
            [
                (
                    "target-storage",
                    "pypi-local",
                    "demo-package/1.2.3/demo-package-1.2.3.tar.gz",
                    7,
                    b"package",
                )
            ],
        )

    def test_migrate_repository_maps_composer_to_php_repository_and_dist_path(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "composer-local": [
                    migrator.ArtifactEntry("acme/demo/1.2.3.zip", 7),
                    migrator.ArtifactEntry("p2/acme/demo.json", 50),
                ]
            },
            contents={
                ("composer-local", "acme/demo/1.2.3.zip"): b"zipdata",
            },
        )
        pkgly = FakePkglyClient()
        repository = migrator.RepositoryDescriptor(
            key="composer-local",
            package_type="composer",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=True,
            path_prefix="",
            dry_run=False,
            retries=0,
            retry_backoff_seconds=0.1,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.skipped_filtered, 1)
        self.assertEqual(
            pkgly.create_calls,
            [
                (
                    "target-storage",
                    "composer-local",
                    "php",
                    None,
                )
            ],
        )
        self.assertEqual(
            pkgly.upload_calls,
            [
                (
                    "target-storage",
                    "composer-local",
                    "dist/acme/demo/1.2.3.zip",
                    7,
                    b"zipdata",
                )
            ],
        )

    def test_migrate_repository_republishes_npm_tarball(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "npm-local": [
                    migrator.ArtifactEntry("@acme/demo/-/demo-1.2.3.tgz", 1),
                    migrator.ArtifactEntry("@acme/demo/index.json", 10),
                ]
            },
            contents={
                (
                    "npm-local",
                    "@acme/demo/-/demo-1.2.3.tgz",
                ): build_npm_tgz({"name": "@acme/demo", "version": "1.2.3"}),
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "npm-local")})
        repository = migrator.RepositoryDescriptor(
            key="npm-local",
            package_type="npm",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_unsupported_artifacts, 1)
        self.assertEqual(len(pkgly.npm_publish_calls), 1)
        self.assertEqual(pkgly.npm_publish_calls[0][2], "@acme/demo")

    def test_migrate_repository_republishes_nuget_package_and_skips_symbols(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "nuget-local": [
                    migrator.ArtifactEntry("Acme.Tools.1.2.3.nupkg", 10),
                    migrator.ArtifactEntry("Acme.Tools.1.2.3.snupkg", 10),
                ]
            },
            contents={
                ("nuget-local", "Acme.Tools.1.2.3.nupkg"): build_nupkg("Acme.Tools", "1.2.3"),
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "nuget-local")})
        repository = migrator.RepositoryDescriptor(
            key="nuget-local",
            package_type="nuget",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_unsupported_artifacts, 1)
        self.assertEqual(pkgly.nuget_publish_calls[0][2:4], ("Acme.Tools", "1.2.3"))

    def test_migrate_repository_republishes_ruby_gem(self):
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "gems-local": [
                    migrator.ArtifactEntry("gems/demo-1.2.3.gem", 7),
                    migrator.ArtifactEntry("specs.4.8.gz", 10),
                ]
            },
            contents={
                ("gems-local", "gems/demo-1.2.3.gem"): b"gemdata",
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "gems-local")})
        repository = migrator.RepositoryDescriptor(
            key="gems-local",
            package_type="gems",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_unsupported_artifacts, 1)
        self.assertEqual(pkgly.ruby_publish_calls[0][2], "demo-1.2.3.gem")

    def test_migrate_repository_republishes_cargo_crate(self):
        crate_bytes = build_cargo_crate("demo-crate", "1.2.3")
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "cargo-local": [
                    migrator.ArtifactEntry("crates/demo-crate/demo-crate-1.2.3.crate", len(crate_bytes)),
                    migrator.ArtifactEntry("index/de/mo/demo-crate", 10),
                ]
            },
            contents={
                ("cargo-local", "crates/demo-crate/demo-crate-1.2.3.crate"): crate_bytes,
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "cargo-local")})
        repository = migrator.RepositoryDescriptor(
            key="cargo-local",
            package_type="cargo",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_unsupported_artifacts, 1)
        self.assertEqual(pkgly.cargo_publish_calls[0][2:4], ("demo-crate", "1.2.3"))

    def test_migrate_repository_republishes_go_module_and_skips_noncanonical_entries(self):
        zip_bytes = build_go_module_zip("github.com/acme/demo", "v1.2.3")
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "go-local": [
                    migrator.ArtifactEntry("github.com/acme/demo/@v/v1.2.3.zip", len(zip_bytes)),
                    migrator.ArtifactEntry("github.com/acme/demo/@v/v1.2.3.mod", 32),
                    migrator.ArtifactEntry("github.com/acme/demo/@v/v1.2.3.info", 22),
                    migrator.ArtifactEntry("github.com/acme/demo/v1.2.3.mod", 20),
                ]
            },
            contents={
                ("go-local", "github.com/acme/demo/@v/v1.2.3.zip"): zip_bytes,
                ("go-local", "github.com/acme/demo/@v/v1.2.3.mod"): b"module github.com/acme/demo\n",
                ("go-local", "github.com/acme/demo/@v/v1.2.3.info"): b'{"Version":"v1.2.3"}',
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "go-local")})
        repository = migrator.RepositoryDescriptor(
            key="go-local",
            package_type="go",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_noncanonical, 1)
        self.assertEqual(pkgly.go_upload_calls[0][2:4], ("github.com/acme/demo", "v1.2.3"))

    def test_migrate_repository_republishes_deb_with_defaults(self):
        deb_bytes = build_deb("hello", "1.0.0", "amd64")
        artifactory = FakeArtifactoryClient(
            repositories=[],
            entries_by_repo={
                "deb-local": [
                    migrator.ArtifactEntry("pool/main/h/hello/hello_1.0.0_amd64.deb", len(deb_bytes)),
                    migrator.ArtifactEntry("dists/stable/Release", 8),
                ]
            },
            contents={
                ("deb-local", "pool/main/h/hello/hello_1.0.0_amd64.deb"): deb_bytes,
            },
        )
        pkgly = FakePkglyClient()
        repository = migrator.RepositoryDescriptor(
            key="deb-local",
            package_type="deb",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=True,
            path_prefix="",
            dry_run=False,
            package_options={"deb_distribution": "stable", "deb_component": "main"},
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(result.skipped_unsupported_artifacts, 1)
        self.assertEqual(
            pkgly.create_calls[0],
            (
                "target-storage",
                "deb-local",
                "deb",
                {
                    "distributions": ["stable"],
                    "components": ["main"],
                    "architectures": ["amd64", "all"],
                },
            ),
        )
        self.assertEqual(pkgly.deb_upload_calls[0][2:4], ("stable", "main"))

    def test_migrate_repository_republishes_docker_images(self):
        config_digest = "sha256:" + "1" * 64
        layer_digest = "sha256:" + "2" * 64
        manifest = {
            "schemaVersion": 2,
            "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
            "config": {
                "mediaType": "application/vnd.docker.container.image.v1+json",
                "size": 5,
                "digest": config_digest,
            },
            "layers": [
                {
                    "mediaType": "application/vnd.docker.image.rootfs.diff.tar.gzip",
                    "size": 5,
                    "digest": layer_digest,
                }
            ],
        }
        manifest_bytes = json.dumps(manifest).encode("utf-8")
        artifactory = FakeArtifactoryClient(
            repositories=[],
            docker_catalog={"docker-local": ["library/demo"]},
            docker_tags={("docker-local", "library/demo"): ["latest"]},
            docker_manifests={
                (
                    "docker-local",
                    "library/demo",
                    "latest",
                ): (manifest_bytes, manifest["mediaType"])
            },
            docker_blobs={
                ("docker-local", "library/demo", config_digest): b"config",
                ("docker-local", "library/demo", layer_digest): b"layer",
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "docker-local")})
        repository = migrator.RepositoryDescriptor(
            key="docker-local",
            package_type="docker",
            repo_type="local",
        )

        result = migrator.migrate_repository(
            repository=repository,
            artifactory=artifactory,
            pkgly=pkgly,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.transferred, 1)
        self.assertEqual(len(pkgly.docker_blob_upload_calls), 2)
        self.assertEqual(len(pkgly.docker_manifest_upload_calls), 1)
        self.assertEqual(pkgly.docker_manifest_upload_calls[0][2:4], ("library/demo", "latest"))


class MultiRepositoryTests(unittest.TestCase):
    def test_migrate_repositories_runs_selected_repos_and_tracks_failures(self):
        repositories = [
            migrator.RepositoryDescriptor(
                key="maven-local",
                package_type="maven",
                repo_type="local",
            ),
            migrator.RepositoryDescriptor(
                key="npm-local",
                package_type="npm",
                repo_type="local",
            ),
        ]
        artifactory = FakeArtifactoryClient(
            repositories=repositories,
            entries_by_repo={
                "maven-local": [
                    migrator.ArtifactEntry("com/acme/app/1.0/app-1.0.jar", 3),
                ],
                "npm-local": [
                    migrator.ArtifactEntry("demo/-/demo-1.0.0.tgz", 3),
                ],
            },
            contents={
                ("maven-local", "com/acme/app/1.0/app-1.0.jar"): b"jar",
                ("npm-local", "demo/-/demo-1.0.0.tgz"): build_npm_tgz({"name": "demo", "version": "1.0.0"}),
            },
        )
        pkgly = FakePkglyClient(
            existing_repositories={
                ("target-storage", "maven-local"),
                ("target-storage", "npm-local"),
            }
        )

        results = migrator.migrate_repositories(
            artifactory=artifactory,
            pkgly=pkgly,
            requested_names=[],
            all_repositories=True,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=False,
            parallelism=3,
            retries=0,
            retry_backoff_seconds=0.1,
            executor_class=FakeExecutor,
            package_options=None,
        )

        self.assertEqual(FakeExecutor.last_max_workers, 3)
        self.assertEqual([result.repository_key for result in results], ["maven-local", "npm-local"])
        self.assertEqual(results[0].status, "success")
        self.assertEqual(results[0].transferred, 1)
        self.assertEqual(results[1].status, "success")
        self.assertEqual(results[1].transferred, 1)

    def test_migrate_repositories_reports_missing_requested_repo(self):
        artifactory = FakeArtifactoryClient(
            repositories=[
                migrator.RepositoryDescriptor(
                    key="maven-local",
                    package_type="maven",
                    repo_type="local",
                )
            ]
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "maven-local")})

        results = migrator.migrate_repositories(
            artifactory=artifactory,
            pkgly=pkgly,
            requested_names=["maven-local", "missing-repo"],
            all_repositories=False,
            target_storage_name="target-storage",
            create_targets=False,
            path_prefix="",
            dry_run=True,
            parallelism=2,
            retries=0,
            retry_backoff_seconds=0.1,
            executor_class=FakeExecutor,
            package_options=None,
        )

        self.assertEqual(len(results), 2)
        self.assertEqual(results[0].repository_key, "maven-local")
        self.assertEqual(results[0].status, "success")
        self.assertEqual(results[1].repository_key, "missing-repo")
        self.assertEqual(results[1].status, "failed")
        self.assertIn("not found", results[1].error)


if __name__ == "__main__":
    unittest.main()
