import io
import sys
import unittest
from pathlib import Path


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
            target_storage_id="storage-uuid",
            path_prefix="",
            dry_run=False,
        )

        self.assertEqual(result.status, "success")
        self.assertEqual(pkgly.create_calls, [("storage-uuid", "maven-local", "maven")])


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
