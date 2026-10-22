// ABOUTME: Builds docker login/tag/push snippets for hosted and proxy setups.
// ABOUTME: Keeps CLI guidance next to the repository URL in the setup panel.
import type { CodeSnippet } from "@/components/core/code/code";
import type { RepositoryWithStorageName } from "@/types/repository";
import { apiURL } from "@/config";
import type { RepositoryRouteTarget } from "@/types/repositoryRoute";

export function createDockerRepositoryName(repository: RepositoryRouteTarget): string {
  return `${new URL(apiURL).host}/${repository.storage_name}/${repository.name}`;
}

export function createDockerPullCommand(name: string, reference: string): string {
  const separator = reference.includes(":") ? "@" : ":";
  return `docker pull ${new URL(apiURL).host}/${name}${separator}${reference}`;
}

export function createDockerSetupSnippets(
  repository: RepositoryWithStorageName,
): Array<CodeSnippet> {
  const host = new URL(apiURL).host;
  const name = createDockerRepositoryName(repository);
  const kind = repository.repository_kind?.toLowerCase();
  const image = `${name}/<image>:<tag>`;

  if (kind === "proxy") {
    return [
      {
        name: "Docker",
        language: "bash",
        key: "docker-proxy",
        code: `${repository.auth_enabled ? `docker login ${host}\n` : ""}docker pull ${image}`,
      },
    ];
  }

  return [
    {
      name: "Docker",
      language: "bash",
      key: "docker-hosted",
      code: `docker login ${host}\ndocker tag <image>:<tag> ${image}\ndocker push ${image}`,
    },
  ];
}
