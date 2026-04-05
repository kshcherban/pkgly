import { beforeAll, describe, expect, it, vi } from "vitest";

beforeAll(() => {
  vi.stubGlobal("localStorage", {
    getItem: vi.fn().mockReturnValue(null),
    setItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
    key: vi.fn(),
    length: 0,
  });
});

describe("router security metadata", () => {
  it("marks home route as auth protected", async () => {
    const router = (await import("@/router")).default;
    const routes = router.getRoutes();
    const home = routes.find((route) => route.name === "home");
    expect(home?.meta?.requiresAuth).toBe(true);
  });

  it("does not expose the removed repositories page route", async () => {
    const router = (await import("@/router")).default;
    const routes = router.getRoutes();
    const repositories = routes.find((route) => route.name === "repositories");
    expect(repositories).toBeUndefined();
  });

  it("redirects admin home to repositories list", async () => {
    const router = (await import("@/router")).default;
    const routes = router.getRoutes();
    const admin = routes.find((route) => route.name === "admin");

    expect(admin?.redirect).toBe("/admin/repositories");
  });
});
