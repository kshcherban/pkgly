import { describe, expect, it } from "vitest";

import {
  createProjectSnippets,
  createSnippetsForPulling,
} from "../MavenRepositoryHelpers";

describe("MavenRepositoryHelpers", () => {
  it("builds repository snippets without loading repository metadata", () => {
    const snippets = createSnippetsForPulling({
      id: "repo-1",
      storage_name: "storage-a",
      storage_id: "storage-id",
      name: "maven-hosted",
      repository_type: "maven",
      active: true,
      visibility: "Public" as const,
      updated_at: "2026-03-26T00:00:00Z",
      created_at: "2026-03-26T00:00:00Z",
      auth_enabled: false,
      storage_usage_bytes: null,
      storage_usage_updated_at: null,
    });

    expect(snippets).toHaveLength(2);
    expect(snippets[0]?.code).toContain("/repositories/storage-a/maven-hosted");
  });

  it("builds project snippets for Maven and Gradle Kotlin", () => {
    const snippets = createProjectSnippets(
      {
        id: "project-1",
        name: "demo-artifact",
        scope: "com.example",
        project_key: "com.example:demo-artifact",
      } as any,
      "1.2.3",
    );

    expect(snippets.map((snippet) => snippet.key)).toEqual(["maven", "gradle-kotlin"]);
    expect(snippets[0]?.code).toContain("<version>1.2.3</version>");
    expect(snippets[1]?.code).toContain('implementation("com.example:demo-artifact:1.2.3")');
  });
});
