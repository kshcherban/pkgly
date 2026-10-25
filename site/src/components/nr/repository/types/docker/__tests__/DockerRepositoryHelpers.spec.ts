// ABOUTME: Verifies Docker setup commands use CLI-compatible registry references.
// ABOUTME: Covers authenticated and anonymous pull guidance alongside hosted pushes.
import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import type { RepositoryWithStorageName } from "@/types/repository";
import CodeCard from "@/components/core/code/CodeCard.vue";
import { createDockerPullCommand, createDockerSetupSnippets } from "../DockerRepositoryHelpers";

function repository(kind: string | null, authEnabled = true): RepositoryWithStorageName {
  return {
    id: "repo-1",
    storage_name: "primary",
    storage_id: "storage-1",
    name: "docker-hosted",
    repository_type: "docker",
    repository_kind: kind,
    active: true,
    visibility: "Private" as RepositoryWithStorageName["visibility"],
    updated_at: "2026-03-26T00:00:00Z",
    created_at: "2026-03-26T00:00:00Z",
    auth_enabled: authEnabled,
    storage_usage_bytes: null,
    storage_usage_updated_at: null,
  };
}

describe("DockerRepositoryHelpers", () => {
  it("renders shell commands in the setup guide", () => {
    const wrapper = mount(CodeCard, {
      props: {
        code: {
          name: "Docker",
          key: "docker",
          language: "bash",
          code: "docker pull localhost:8000/test/proxy/nginx:alpine",
        },
      },
    });
    expect(wrapper.get("code").text()).toBe("docker pull localhost:8000/test/proxy/nginx:alpine");
  });
  it("builds login/tag/push guidance for hosted registries", () => {
    const snippets = createDockerSetupSnippets(repository("hosted"));

    expect(snippets).toHaveLength(1);
    expect(snippets[0]?.code).toContain("docker login");
    expect(snippets[0]?.code).toContain("docker tag");
    expect(snippets[0]?.code).toContain("docker push");
    expect(snippets[0]?.code?.split("\n")).toEqual([
      "docker login localhost:3000",
      "docker tag <image>:<tag> localhost:3000/primary/docker-hosted/<image>:<tag>",
      "docker push localhost:3000/primary/docker-hosted/<image>:<tag>",
    ]);
  });

  it("builds login/pull guidance for proxy registries", () => {
    const snippets = createDockerSetupSnippets(repository("proxy"));

    expect(snippets).toHaveLength(1);
    expect(snippets[0]?.code?.split("\n")).toEqual([
      "docker login localhost:3000",
      "docker pull localhost:3000/primary/docker-hosted/<image>:<tag>",
    ]);
    expect(snippets[0]?.code).not.toContain("docker push");
  });
  it("offers an anonymous pull command when proxy authentication is disabled", () => {
    expect(createDockerSetupSnippets(repository("proxy", false))[0]?.code).toBe(
      "docker pull localhost:3000/primary/docker-hosted/<image>:<tag>",
    );
  });
});

describe("Docker pull commands", () => {
  it.each([
    [
      "test/proxy/nginx",
      "stable-alpine",
      "docker pull localhost:3000/test/proxy/nginx:stable-alpine",
    ],
    [
      "test/proxy/library/alpine",
      "sha256:abc123",
      "docker pull localhost:3000/test/proxy/library/alpine@sha256:abc123",
    ],
  ])("formats %s at %s", (name, reference, expected) => {
    expect(createDockerPullCommand(name, reference)).toBe(expected);
  });
});
