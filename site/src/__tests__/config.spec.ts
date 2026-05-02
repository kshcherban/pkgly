// ABOUTME: Tests browser-derived API and WebSocket URL configuration.
// ABOUTME: Covers refresh paths where document.baseURI would point at a nested route.
import { afterEach, describe, expect, it, vi } from "vitest";

async function loadConfig(path: string, baseHref?: string, viteApiUrl?: string) {
  vi.resetModules();
  vi.unstubAllEnvs();
  vi.spyOn(console, "log").mockImplementation(() => undefined);

  window.history.replaceState({}, "", path);
  document.head.innerHTML = "";

  if (baseHref !== undefined) {
    const base = document.createElement("base");
    base.setAttribute("href", baseHref);
    document.head.appendChild(base);
  }

  if (viteApiUrl !== undefined) {
    vi.stubEnv("VITE_API_URL", viteApiUrl);
  }

  return import("@/config");
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  document.head.innerHTML = "";
});

describe("apiURL", () => {
  it("defaults to the site root when no base href is configured on a refreshed nested path", async () => {
    const config = await loadConfig("/admin/repository/22222222-0000-0000-0000-000000000001");

    expect(config.apiURL).toBe(`${window.location.origin}/`);
  });

  it("defaults to the site root when base href is blank on a refreshed nested path", async () => {
    const config = await loadConfig(
      "/admin/system/sso",
      "",
    );

    expect(config.apiURL).toBe(`${window.location.origin}/`);
  });

  it("uses a configured non-empty base href", async () => {
    const config = await loadConfig(
      "/admin/system/sso",
      "https://repo.pkgly.dev/pkgly/",
    );

    expect(config.apiURL).toBe("https://repo.pkgly.dev/pkgly/");
  });

  it("uses VITE_API_URL when present", async () => {
    const config = await loadConfig(
      "/admin/system/sso",
      "",
      "https://api.pkgly.dev/",
    );

    expect(config.apiURL).toBe("https://api.pkgly.dev/");
  });
});
