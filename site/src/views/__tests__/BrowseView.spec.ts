import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia, type Pinia } from "pinia";

const mockLocalStorage = {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
  clear: () => undefined,
};

Object.defineProperty(globalThis, "localStorage", {
  value: mockLocalStorage,
  writable: true,
});

vi.mock("@/router", () => ({
  default: {
    currentRoute: {
      value: {
        params: {
          id: "repo-php",
          catchAll: undefined,
        },
      },
    },
    push: vi.fn(),
  },
}));

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

vi.mock("@vue/devtools-kit", () => ({}));

const http = (await import("@/http")).default as { get: vi.Mock };

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  readyState = 1;
  sent: string[] = [];
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  url: string;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(payload: string) {
    this.sent.push(payload);
  }

  close() {
    if (this.onclose) {
      this.onclose();
    }
  }

  emitOpen() {
    this.onopen?.(new Event("open"));
  }
}

describe("BrowseView", () => {
  let pinia: Pinia;

  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket as unknown as typeof WebSocket);
    pinia = createPinia();
    setActivePinia(pinia);
    http.get.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders the browse list for PHP repositories at the root path", async () => {
    http.get.mockImplementation((url: string) => {
      if (url === "/api/repository/repo-php") {
        return Promise.resolve({
          data: {
            id: "repo-php",
            name: "composer-hosted",
            storage_id: "storage-123",
            storage_name: "primary",
            repository_type: "php",
            repository_kind: "hosted",
            visibility: "Private",
            active: true,
            updated_at: "2025-12-08T12:00:00Z",
            created_at: "2025-12-08T10:00:00Z",
            auth_enabled: true,
            storage_usage_bytes: null,
            storage_usage_updated_at: null,
          },
        });
      }
      return Promise.reject(new Error(`Unhandled URL ${url}`));
    });

    const BrowseView = (await import("@/views/BrowseView.vue")).default;

    const wrapper = mount(BrowseView, {
      global: {
        plugins: [pinia],
        stubs: {
          BrowseHeader: {
            template: "<div data-testid='browse-header'></div>",
          },
          BrowseList: {
            template: "<div data-testid='browse-list'></div>",
          },
          BrowseProject: {
            template: "<div data-testid='browse-project'></div>",
          },
        },
      },
    });

    await flushPromises();

    MockWebSocket.instances[0]?.emitOpen();
    await flushPromises();

    expect(wrapper.find("[data-testid='packages-public']").exists()).toBe(false);
    expect(MockWebSocket.instances[0]?.sent).toContain(
      JSON.stringify({ type: "ListDirectory", data: "" }),
    );
  });

  it("renders the browse list once directory entries are available", async () => {
    http.get.mockImplementation((url: string) => {
      if (url === "/api/repository/repo-php") {
        return Promise.resolve({
          data: {
            id: "repo-php",
            name: "composer-hosted",
            storage_id: "storage-123",
            storage_name: "primary",
            repository_type: "php",
            repository_kind: "hosted",
            visibility: "Private",
            active: true,
            updated_at: "2025-12-08T12:00:00Z",
            created_at: "2025-12-08T10:00:00Z",
            auth_enabled: true,
            storage_usage_bytes: null,
            storage_usage_updated_at: null,
          },
        });
      }
      return Promise.reject(new Error(`Unhandled URL ${url}`));
    });

    const BrowseView = (await import("@/views/BrowseView.vue")).default;

    const wrapper = mount(BrowseView, {
      global: {
        plugins: [pinia],
        stubs: {
          BrowseHeader: {
            template: "<div data-testid='browse-header'></div>",
          },
          BrowseList: {
            template: "<div data-testid='browse-list'></div>",
          },
          BrowseProject: {
            template: "<div data-testid='browse-project'></div>",
          },
        },
      },
    });

    await flushPromises();
    MockWebSocket.instances[0]?.emitOpen();
    MockWebSocket.instances[0]?.onmessage?.(
      new MessageEvent("message", {
        data: JSON.stringify({
          type: "OpenedDirectory",
          data: {
            number_of_files: 1,
            project_resolution: null,
          },
        }),
      }),
    );
    await flushPromises();

    expect(wrapper.find("[data-testid='browse-list']").exists()).toBe(true);
  });

  it("renders the browse list for Ruby repositories at the root path", async () => {
    http.get.mockImplementation((url: string) => {
      if (url === "/api/repository/repo-ruby") {
        return Promise.resolve({
          data: {
            id: "repo-ruby",
            name: "rubygems-hosted",
            storage_id: "storage-123",
            storage_name: "primary",
            repository_type: "ruby",
            repository_kind: "hosted",
            visibility: "Private",
            active: true,
            updated_at: "2025-12-08T12:00:00Z",
            created_at: "2025-12-08T10:00:00Z",
            auth_enabled: true,
            storage_usage_bytes: null,
            storage_usage_updated_at: null,
          },
        });
      }
      return Promise.reject(new Error(`Unhandled URL ${url}`));
    });

    const router = (await import("@/router")).default;
    router.currentRoute.value.params.id = "repo-ruby";
    router.currentRoute.value.params.catchAll = undefined;

    const BrowseView = (await import("@/views/BrowseView.vue")).default;

    const wrapper = mount(BrowseView, {
      global: {
        plugins: [pinia],
        stubs: {
          BrowseHeader: {
            template: "<div data-testid='browse-header'></div>",
          },
          BrowseList: {
            template: "<div data-testid='browse-list'></div>",
          },
          BrowseProject: {
            template: "<div data-testid='browse-project'></div>",
          },
        },
      },
    });

    await flushPromises();

    MockWebSocket.instances[0]?.emitOpen();
    await flushPromises();

    expect(wrapper.find("[data-testid='packages-public']").exists()).toBe(false);
    expect(MockWebSocket.instances[0]?.sent).toContain(
      JSON.stringify({ type: "ListDirectory", data: "" }),
    );
  });
});
