import { expect, it, describe } from "vitest";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.resolve(__dirname, "..", "..", "..");
const legacyComponentPath = path.resolve(__dirname, "..", "BaseSwitch.vue");

function collectFiles(dir: string, extensions: Array<string>): Array<string> {
  const entries = readdirSync(dir, { withFileTypes: true });
  const files: Array<string> = [];

  for (const entry of entries) {
    const resolvedPath = path.resolve(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(resolvedPath, extensions));
    } else if (extensions.includes(path.extname(entry.name))) {
      files.push(resolvedPath);
    }
  }

  return files;
}

describe("legacy switch styling", () => {
  it("removes the legacy BaseSwitch component", () => {
    expect(existsSync(legacyComponentPath)).toBe(false);
  });

  it("prevents legacy switch and slider classes", () => {
    const files = collectFiles(srcDir, [".vue", ".scss"]);
    const offenders: Array<string> = [];

    for (const filePath of files) {
      const content = readFileSync(filePath, "utf8");
      const hasLegacyMarkup = /class\s*=\s*["']switch["']/.test(content) ||
        /class\s*=\s*["']slider["']/.test(content);
      const hasLegacySelector = path.extname(filePath) === ".scss" && /\.slider\b/.test(content);

      if (hasLegacyMarkup || hasLegacySelector) {
        offenders.push(path.relative(srcDir, filePath));
      }
    }

    expect(
      offenders,
      `Legacy switch styling still present in: ${offenders.join(", ")}`,
    ).toEqual([]);
  });
});
