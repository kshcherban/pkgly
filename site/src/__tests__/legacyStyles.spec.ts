import { describe, expect, it } from "vitest";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const sourceRoot = join(process.cwd(), "src");

async function listFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];

  for (const entry of entries) {
    const fullPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFiles(fullPath)));
    } else if (
      fullPath.endsWith(".vue") ||
      fullPath.endsWith(".scss") ||
      fullPath.endsWith(".sass") ||
      fullPath.endsWith(".css")
    ) {
      files.push(fullPath);
    }
  }

  return files;
}

async function findMatches(search: string | RegExp): Promise<string[]> {
  const files = await listFiles(sourceRoot);
  const matched: string[] = [];

  for (const file of files) {
    const contents = await readFile(file, "utf-8");
    if (typeof search === "string") {
      if (contents.includes(search)) {
        matched.push(file);
      }
    } else if (search.test(contents)) {
      matched.push(file);
    }
  }

  return matched;
}

describe("legacy styling guardrails", () => {
  it("has removed all uses of the deprecated nr-button classes", async () => {
    const matches = await findMatches("nr-button");
    expect(matches).toEqual([]);
  });

  it("does not use Sass @import directives in project styles", async () => {
    const files = await listFiles(sourceRoot);
    const offenders: string[] = [];

    for (const file of files) {
      const contents = await readFile(file, "utf-8");
      const hasBannedImport = contents
        .split(/\r?\n/)
        .some((line) => {
          const trimmed = line.trim();
          if (!trimmed.startsWith("@import")) {
            return false;
          }
          return !trimmed.startsWith("@import url");
        });
      if (hasBannedImport) {
        offenders.push(file);
      }
    }

    expect(offenders).toEqual([]);
  });
});
