import io
import sys
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import artifactory_to_pkgly as migrator  # noqa: E402


class FakeArtifactoryClient:
    def __init__(self, repositories, entries_by_repo=None, contents=None):
        self._repositories = list(repositories)
        self._entries_by_repo = entries_by_repo or {}
        self._contents = contents or {}
        self.list_repositories_calls = 0
        self.list_files_calls = []
        self.open_file_calls = []

    def list_repositories(self):
        self.list_repositories_calls += 1
        return list(self._repositories)

    def list_files(self, repository_key, path_prefix=""):
        self.list_files_calls.append((repository_key, path_prefix))
        return list(self._entries_by_repo.get(repository_key, []))

    def open_file(self, repository_key, path):
        self.open_file_calls.append((repository_key, path))
        return io.BytesIO(self._contents[(repository_key, path)])


class FakePkglyClient:
    def __init__(self, existing_repositories=None, existing_artifacts=None):
        self._existing_repositories = set(existing_repositories or [])
        self._existing_artifacts = set(existing_artifacts or [])
        self.repository_exists_calls = []
        self.create_calls = []
        self.artifact_exists_calls = []
        self.upload_calls = []

    def repository_exists(self, storage_name, repository_name):
        self.repository_exists_calls.append((storage_name, repository_name))
        return (storage_name, repository_name) in self._existing_repositories

    def create_repository(self, storage_id, repository_name, package_type):
        self.create_calls.append((storage_id, repository_name, package_type))
        self._existing_repositories.add(("target-storage", repository_name))

    def artifact_exists(self, storage_name, repository_name, relative_path):
        self.artifact_exists_calls.append((storage_name, repository_name, relative_path))
        return (storage_name, repository_name, relative_path) in self._existing_artifacts

    def upload_file(self, storage_name, repository_name, relative_path, stream, size):
        self.upload_calls.append(
            (storage_name, repository_name, relative_path, size, stream.read())
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
            target_storage_id=None,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(result.discovered, 3)
        self.assertEqual(result.skipped_filtered, 1)
        self.assertEqual(result.skipped_existing, 1)
        self.assertEqual(result.transferred, 1)
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

    def test_migrate_repository_marks_unsupported_package_type_failed(self):
        artifactory = FakeArtifactoryClient(repositories=[])
        pkgly = FakePkglyClient()
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
            target_storage_id=None,
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "failed")
        self.assertIn("unsupported package type", result.error)
        self.assertEqual(pkgly.upload_calls, [])

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
            target_storage_id=None,
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
            target_storage_id="123e4567-e89b-12d3-a456-426614174000",
            path_prefix="",
            dry_run=False,
            retries=0,
            retry_backoff_seconds=0.1,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(
            pkgly.create_calls,
            [("123e4567-e89b-12d3-a456-426614174000", "maven-local", "maven")],
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
            target_storage_id=None,
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
                "npm-local": [],
            },
            contents={
                ("maven-local", "com/acme/app/1.0/app-1.0.jar"): b"jar",
            },
        )
        pkgly = FakePkglyClient(existing_repositories={("target-storage", "maven-local")})

        results = migrator.migrate_repositories(
            artifactory=artifactory,
            pkgly=pkgly,
            requested_names=[],
            all_repositories=True,
            target_storage_name="target-storage",
            create_targets=False,
            target_storage_id=None,
            path_prefix="",
            dry_run=False,
            parallelism=3,
            retries=0,
            retry_backoff_seconds=0.1,
            executor_class=FakeExecutor,
        )

        self.assertEqual(FakeExecutor.last_max_workers, 3)
        self.assertEqual([result.repository_key for result in results], ["maven-local", "npm-local"])
        self.assertEqual(results[0].status, "success")
        self.assertEqual(results[0].transferred, 1)
        self.assertEqual(results[1].status, "failed")
        self.assertIn("unsupported package type", results[1].error)

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
            target_storage_id=None,
            path_prefix="",
            dry_run=True,
            parallelism=2,
            retries=0,
            retry_backoff_seconds=0.1,
            executor_class=FakeExecutor,
        )

        self.assertEqual(len(results), 2)
        self.assertEqual(results[0].repository_key, "maven-local")
        self.assertEqual(results[0].status, "success")
        self.assertEqual(results[1].repository_key, "missing-repo")
        self.assertEqual(results[1].status, "failed")
        self.assertIn("not found", results[1].error)


if __name__ == "__main__":
    unittest.main()
