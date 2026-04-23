import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RouteLocationNormalized } from "vue-router";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

import http from "@/http";
import { installAwareAuthGuard } from "@/router/installGuard";

function route(overrides: Partial<RouteLocationNormalized>): RouteLocationNormalized {
  return {
    fullPath: "/",
    meta: {},
    ...overrides,
  } as RouteLocationNormalized;
}

describe("installAwareAuthGuard", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    setActivePinia(createPinia());
  });

  it("redirects protected routes to install before refreshing the current user", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    (http.get as vi.Mock).mockImplementation((path: string) => {
      if (path === "/api/info") {
        return Promise.resolve({
          data: {
            is_installed: false,
          },
        });
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });

    const result = await installAwareAuthGuard(
      route({
        fullPath: "/",
        meta: {
          requiresAuth: true,
        },
      }),
      pinia,
    );

    expect(result).toEqual({ name: "AdminInstall" });
    expect(http.get).toHaveBeenCalledTimes(1);
    expect(http.get).toHaveBeenCalledWith("/api/info");
  });
});
