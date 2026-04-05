import { apiURL } from "@/config";
import { fixCurrentPath } from "./browse";

export interface RepositoryRouteTarget {
  storage_name: string;
  name: string;
}

export interface BrowseFileRouteTarget extends RepositoryRouteTarget {
  repository_type?: string;
}

export function createRepositoryRoute(
  repository: RepositoryRouteTarget,
  route?: string,
): string {
  let backend = apiURL;
  if (backend.endsWith("/")) {
    backend = backend.substring(0, backend.length - 1);
  }
  if (route === undefined) {
    return `${backend}/repositories/${repository.storage_name}/${repository.name}`;
  }
  return `${backend}/repositories/${repository.storage_name}/${repository.name}/${route}`;
}

function normalizeRoute(path: string): string {
  return fixCurrentPath(path);
}

function buildRelativeFilePath(currentPath: string, fileName: string): string {
  const normalizedPath = normalizeRoute(currentPath);
  if (normalizedPath.length === 0) {
    return fileName;
  }
  return `${normalizedPath}/${fileName}`;
}

function createDockerBrowseFileRoute(
  repository: BrowseFileRouteTarget,
  currentPath: string,
  fileName: string,
): string | null {
  if (fileName.endsWith(".nr-docker-tagmeta")) {
    return null;
  }

  const normalizedPath = normalizeRoute(currentPath);
  if (normalizedPath.startsWith("v2/")) {
    return createRepositoryRoute(repository, buildRelativeFilePath(normalizedPath, fileName));
  }

  const logicalRoot = `${repository.storage_name}/${repository.name}`;
  const imagePath = normalizedPath === logicalRoot
    ? ""
    : normalizedPath.startsWith(`${logicalRoot}/`)
      ? normalizedPath.slice(logicalRoot.length + 1)
      : normalizedPath;

  if (imagePath.length === 0) {
    return null;
  }

  return createRepositoryRoute(repository, `v2/${imagePath}/manifests/${fileName}`);
}

export function createBrowseFileRoute(
  repository: BrowseFileRouteTarget,
  currentPath: string,
  fileName: string,
): string | null {
  if (repository.repository_type?.toLowerCase() === "docker") {
    return createDockerBrowseFileRoute(repository, currentPath, fileName);
  }
  return createRepositoryRoute(repository, buildRelativeFilePath(currentPath, fileName));
}
