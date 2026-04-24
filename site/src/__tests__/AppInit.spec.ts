import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { defineComponent, h } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

import App from "@/App.vue";
import http from "@/http";
import router from "@/router";

const passthroughStub = defineComponent({
  setup(_, { slots }) {
    return () => h("div", slots.default?.());
  },
});

const routerViewStub = defineComponent({
  setup(_, { slots }) {
    const component = defineComponent({
      setup() {
        return () => h("div", "route");
      },
    });
    return () =>
      slots.default?.({
        Component: component,
        route: {
          fullPath: "/",
          meta: {},
        },
      });
  },
});

const adminRouterViewStub = defineComponent({
  setup(_, { slots }) {
    const component = defineComponent({
      setup() {
        return () => h("div", "admin route");
      },
    });
    const sideBar = defineComponent({
      setup() {
        return () => h("nav", "admin nav");
      },
    });
    return () =>
      slots.default?.({
        Component: component,
        route: {
          fullPath: "/admin/repositories",
          meta: { sideBar },
        },
      });
  },
});

describe("App.vue initialization", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    setActivePinia(createPinia());
  });

  it("redirects fresh installs without refreshing the current user", async () => {
    const push = vi.spyOn(router, "push").mockResolvedValue(undefined);
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

    mount(App, {
      global: {
        stubs: {
          AppBar: true,
          GlobalAlerts: true,
          RouterView: routerViewStub,
          "v-app": passthroughStub,
          "v-main": passthroughStub,
          "v-slide-x-transition": passthroughStub,
        },
      },
    });

    await flushPromises();

    expect(push).toHaveBeenCalledWith("/admin/install");
    expect(http.get).toHaveBeenCalledTimes(1);
    expect(http.get).toHaveBeenCalledWith("/api/info");
  });

  it("marks admin side-bar layouts for admin-specific card styling", async () => {
    (http.get as vi.Mock).mockImplementation((path: string) => {
      if (path === "/api/info") {
        return Promise.resolve({
          data: {
            is_installed: true,
          },
        });
      }
      if (path === "/api/user/me") {
        return Promise.resolve({
          data: {
            user: { username: "admin" },
            session: {
              expires: "2026-04-25T00:00:00Z",
              created: "2026-04-24T00:00:00Z",
            },
          },
        });
      }
      return Promise.reject(new Error(`unexpected request: ${path}`));
    });

    const wrapper = mount(App, {
      global: {
        stubs: {
          AppBar: true,
          GlobalAlerts: true,
          RouterView: adminRouterViewStub,
          "v-app": passthroughStub,
          "v-main": passthroughStub,
          "v-slide-x-transition": passthroughStub,
        },
      },
    });

    await flushPromises();

    expect(wrapper.get(".contentWithSideBar").classes()).toContain("contentWithSideBar--admin");
  });
});
