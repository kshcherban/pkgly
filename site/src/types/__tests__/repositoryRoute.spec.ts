import { describe, expect, it } from "vitest";

import { createBrowseFileRoute, createRepositoryRoute } from "@/types/repositoryRoute";

describe("repositoryRoute", () => {
  it("keeps direct repository routes unchanged", () => {
    expect(
      createRepositoryRoute(
        {
          storage_name: "local",
          name: "dockerhub",
        },
        "v2/postgres/manifests/18",
      ),
    ).toBe("http://localhost:3000/repositories/local/dockerhub/v2/postgres/manifests/18");
  });

  it("translates Docker browse paths to manifest download routes", () => {
    expect(
      createBrowseFileRoute(
        {
          storage_name: "local",
          name: "dockerhub",
          repository_type: "docker",
        },
        "local/dockerhub/postgres",
        "18",
      ),
    ).toBe("http://localhost:3000/repositories/local/dockerhub/v2/postgres/manifests/18");
  });

  it("returns null for Docker tag metadata files", () => {
    expect(
      createBrowseFileRoute(
        {
          storage_name: "local",
          name: "dockerhub",
          repository_type: "docker",
        },
        "local/dockerhub/postgres",
        "18.nr-docker-tagmeta",
      ),
    ).toBeNull();
  });

  it("passes through non-Docker browse paths", () => {
    expect(
      createBrowseFileRoute(
        {
          storage_name: "local",
          name: "npmjs",
          repository_type: "npm",
        },
        "packages/demo",
        "demo-1.0.0.tgz",
      ),
    ).toBe("http://localhost:3000/repositories/local/npmjs/packages/demo/demo-1.0.0.tgz");
  });
});
