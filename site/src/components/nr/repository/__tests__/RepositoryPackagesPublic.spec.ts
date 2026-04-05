import { config, flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h, nextTick } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const routerPush = vi.fn();

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

vi.mock("vue-router", () => ({
  useRouter: () => ({
    push: routerPush,
  }),
}));

import RepositoryPackagesPublic from "@/components/nr/repository/RepositoryPackagesPublic.vue";
import http from "@/http";

function createPackages(items: any[] = [], total = items.length, headers: Record<string, string> = {}) {
  return {
    data: {
      items,
      total_packages: total,
    },
    headers,
  };
}

function createLocalStorageStub() {
  let store: Record<string, string> = {};
  return {
    getItem(key: string) {
      return Object.prototype.hasOwnProperty.call(store, key) ? store[key] : null;
    },
    setItem(key: string, value: string) {
      store[key] = String(value);
    },
    removeItem(key: string) {
      delete store[key];
    },
    clear() {
      store = {};
    },
    key(index: number) {
      return Object.keys(store)[index] ?? null;
    },
    get length() {
      return Object.keys(store).length;
    },
  };
}

const vBtnStub = defineComponent({
    name: "VBtnStub",
    emits: ["click"],
    setup(_, { slots, emit }) {
      return () =>
        h(
          "button",
          {
            "data-stub": "v-btn",
            type: "button",
            onClick: (event: Event) => emit("click", event),
          },
          slots.default?.(),
        );
    },
  });

const vSelectStub = defineComponent({
    name: "VSelectStub",
    props: {
      modelValue: {
        type: [String, Number, Array, Object],
        default: undefined,
      },
      items: {
        type: Array,
        default: () => [],
      },
    },
    emits: ["update:modelValue"],
    setup(props, { emit }) {
      return () =>
        h(
          "select",
          {
            "data-stub": "v-select",
            value: props.modelValue as any,
            onChange: (event: Event) => {
              const target = event.target as HTMLSelectElement;
              emit("update:modelValue", target.value);
            },
          },
          (props.items as any[]).map((item) =>
            h("option", { value: item }, item),
          ),
        );
    },
  });

const vuetifyStubs = {
  "v-btn": vBtnStub,
  VBtn: vBtnStub,
  "v-select": vSelectStub,
  VSelect: vSelectStub,
};

config.global.stubs = {
  ...config.global.stubs,
  ...vuetifyStubs,
};

describe("RepositoryPackagesPublic.vue", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.stubGlobal("localStorage", createLocalStorageStub());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("requests packages with per_page 100 by default", async () => {
    (http.get as vi.Mock).mockResolvedValue(createPackages());

    mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-123",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(http.get).toHaveBeenCalledWith(
      "/api/repository/repo-123/packages",
      expect.objectContaining({
        params: expect.objectContaining({
          per_page: 100,
          page: 1,
        }),
      }),
    );
  });

  it("sorts the current page by column when headers are clicked", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-beta",
          name: "Beta",
          size: 2048,
          cache_path: "cache/pkg-beta",
          modified: "2025-11-05T09:30:00Z",
        },
        {
          package: "pkg-alpha",
          name: "Alpha",
          size: 1024,
          cache_path: "cache/pkg-alpha",
          modified: "2025-11-06T11:45:00Z",
        },
        {
          package: "pkg-gamma",
          name: "Gamma",
          size: 1536,
          cache_path: "cache/pkg-gamma",
          modified: "2025-11-04T18:15:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-123",
        repositoryType: "python",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const rowOrder = () =>
      wrapper
        .findAll('[data-testid="package-row"]')
        .map((row) => row.find('[data-testid="package-cell"]').text().trim());

    expect(rowOrder()).toEqual(["pkg-beta", "pkg-alpha", "pkg-gamma"]);

    await wrapper.get('[data-testid="sort-size"]').trigger("click");
    await flushPromises();
    expect(rowOrder()).toEqual(["pkg-alpha", "pkg-gamma", "pkg-beta"]);

    await wrapper.get('[data-testid="sort-size"]').trigger("click");
    await flushPromises();
    expect(rowOrder()).toEqual(["pkg-beta", "pkg-gamma", "pkg-alpha"]);
  });

  it("keeps package counts visible and avoids refetching when sorting the current page", async () => {
    const pendingResponse = new Promise(() => {});
    (http.get as vi.Mock)
      .mockResolvedValueOnce(
        createPackages([
          {
            package: "pkg-beta",
            name: "Beta",
            size: 2048,
            cache_path: "cache/pkg-beta",
            modified: "2025-11-05T09:30:00Z",
          },
          {
            package: "pkg-alpha",
            name: "Alpha",
            size: 1024,
            cache_path: "cache/pkg-alpha",
            modified: "2025-11-06T11:45:00Z",
          },
          {
            package: "pkg-gamma",
            name: "Gamma",
            size: 1536,
            cache_path: "cache/pkg-gamma",
            modified: "2025-11-04T18:15:00Z",
          },
        ]),
      )
      .mockReturnValueOnce(pendingResponse);

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-123",
        repositoryType: "python",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find(".packages__counts").text()).toContain("3 package(s)");
    expect(http.get).toHaveBeenCalledTimes(1);

    await wrapper.get('[data-testid="sort-size"]').trigger("click");
    await nextTick();

    expect(http.get).toHaveBeenCalledTimes(1);
    expect(wrapper.find(".packages__counts").exists()).toBe(true);
    expect(wrapper.find(".packages__counts").text()).toContain("3 package(s)");
    expect(wrapper.find(".packages__state").exists()).toBe(false);
  });

  it("filters packages with the inline search input", async () => {
    (http.get as vi.Mock)
      .mockResolvedValueOnce(
        createPackages([
          {
            package: "pkg-one",
            name: "One",
            size: 1024,
            cache_path: "cache/pkg-one",
            modified: "2025-11-05T09:30:00Z",
          },
          {
            package: "pkg-two",
            name: "Two",
            size: 2048,
            cache_path: "cache/pkg-two",
            modified: "2025-11-05T11:30:00Z",
          },
        ]),
      )
      .mockResolvedValueOnce(
        createPackages([
          {
            package: "pkg-two",
            name: "Two",
            size: 2048,
            cache_path: "cache/pkg-two",
            modified: "2025-11-05T11:30:00Z",
          },
        ]),
      );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-xyz",
        repositoryType: "python",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();
    expect(wrapper.findAll('[data-testid="package-row"]')).toHaveLength(2);

    await wrapper.get('[data-testid="packages-search-input"]').setValue("two");
    await flushPromises();

    expect(http.get).toHaveBeenLastCalledWith(
      "/api/repository/repo-xyz/packages",
      expect.objectContaining({
        params: expect.objectContaining({
          q: "two",
        }),
      }),
    );

    const rows = wrapper.findAll('[data-testid="package-row"]');
    expect(rows).toHaveLength(1);
    expect(rows[0].find('[data-testid="package-cell"]').text()).toBe("pkg-two");
  });

  it("labels name column as Version for PHP proxy repositories", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "acme/example",
          name: "1.2.3",
          size: 1024,
          cache_path: "dist/acme/example/1.2.3/pkg-1.2.3.zip",
          modified: "2025-12-10T12:00:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-php-proxy",
        repositoryType: "php",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const nameHeader = wrapper.find('th[data-column="name"]');
    expect(nameHeader.exists()).toBe(true);
    expect(nameHeader.text()).toContain("Version");
  });

  it("renders an explicit narrow size column definition", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-one",
          name: "1.0.0",
          size: 1024,
          cache_path: "cache/pkg-one",
          modified: "2025-11-05T09:30:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-size",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const sizeCol = wrapper.get('col.packages__col--size');
    expect(sizeCol.attributes("style")).toContain("width: 1%");
    expect(wrapper.get('[data-testid="package-row"] td.packages__column--size').text()).toContain("KB");
  });

  it("opens docker package browse from the existing package column", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "local/dockerhub/postgres",
          name: "sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f",
          size: 1024,
          cache_path: "v2/local/dockerhub/postgres/manifests/sha256:1090bc3a8ccfb0b55f78a494d76f8d603434f7e4553543d6e807bc7bd6bbd17f",
          modified: "2025-11-06T11:45:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-123",
        repositoryType: "docker",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find('th[data-column="repository"]').exists()).toBe(false);
    expect(wrapper.get('th[data-column="package"]').text()).toContain("Repository");

    await wrapper.get('[data-testid="package-cell"]').trigger("click");

    expect(routerPush).toHaveBeenCalledWith({
      name: "Browse",
      params: {
        id: "repo-123",
        catchAll: "local/dockerhub/postgres",
      },
    });
  });

  it("falls back to repository root browse when package path has no parent directory", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-root",
          name: "1.0.0",
          size: 512,
          cache_path: "pkg-root.tgz",
          modified: "2025-11-06T11:45:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-root",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();
    await wrapper.get('[data-testid="package-cell"]').trigger("click");

    expect(routerPush).toHaveBeenCalledWith({
      name: "Browse",
      params: {
        id: "repo-root",
        catchAll: "",
      },
    });
  });

  it("renders indexing warning headers", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([], 0, { "x-pkgly-warning": "Repository indexing in progress" }),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-123",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const warning = wrapper.find('[data-testid="public-packages-indexing-warning"]');
    expect(warning.exists()).toBe(true);
    expect(warning.text()).toContain("Repository indexing in progress");
  });

  it("suppresses indexing warning headers for Ruby proxy repositories", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([], 0, { "x-pkgly-warning": "Repository awaiting indexing" }),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-ruby-proxy",
        repositoryType: "ruby",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const warning = wrapper.find('[data-testid="public-packages-indexing-warning"]');
    expect(warning.exists()).toBe(false);
  });

  it("persists column visibility preferences per repository", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-alpha",
          name: "Alpha",
          size: 1024,
          cache_path: "cache/pkg-alpha",
          modified: "2025-11-06T11:45:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-ABC",
        repositoryType: "python",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    await wrapper.get('[data-testid="packages-config-toggle"]').trigger("click");
    const pathToggle = wrapper.get('[data-testid="toggle-path"]');
    expect((pathToggle.element as HTMLInputElement).checked).toBe(true);

    await pathToggle.setValue(false);
    await flushPromises();

    expect(wrapper.find('th[data-column="path"]').exists()).toBe(false);

    wrapper.unmount();

    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-beta",
          name: "Beta",
          size: 512,
          cache_path: "cache/pkg-beta",
          modified: "2025-11-01T08:00:00Z",
        },
      ]),
    );

    const wrapperAgain = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-ABC",
        repositoryType: "python",
        repositoryKind: "proxy",
      },
    });

    await flushPromises();
    expect(wrapperAgain.find('th[data-column="path"]').exists()).toBe(false);
  });

  it("hides blob digest and manifest path by default for Docker repositories", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "nginx",
          name: "alpine",
          size: 10332,
          cache_path: "v2/nginx/manifests/alpine",
          modified: "2025-11-06T11:45:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-docker-defaults",
        repositoryType: "docker",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find('th[data-column="digest"]').exists()).toBe(false);
    expect(wrapper.find('th[data-column="path"]').exists()).toBe(false);

    await wrapper.get('[data-testid="packages-config-toggle"]').trigger("click");
    const digestToggle = wrapper.get('[data-testid="toggle-digest"]');
    const pathToggle = wrapper.get('[data-testid="toggle-path"]');
    expect((digestToggle.element as HTMLInputElement).checked).toBe(false);
    expect((pathToggle.element as HTMLInputElement).checked).toBe(false);
  });

  it("keeps column resizers after navigating to the next page", async () => {
    vi.useFakeTimers();
    const host = document.createElement("div");
    document.body.appendChild(host);
    try {
      const buildPage = (page: number) =>
        Array.from({ length: 100 }, (_, idx) => ({
          package: `pkg-${page}-${idx}`,
          name: `Name ${page}-${idx}`,
          size: 1024 + idx,
          cache_path: `cache/pkg-${page}-${idx}`,
          modified: "2025-12-01T00:00:00Z",
        }));

      (http.get as vi.Mock).mockImplementation((_url: string, config?: any) => {
        const page = Number(config?.params?.page ?? 1);
        if (page === 1) {
          return Promise.resolve(createPackages(buildPage(1), 200));
        }
        if (page === 2) {
          return Promise.resolve(createPackages(buildPage(2), 200));
        }
        return Promise.resolve(createPackages([], 200));
      });

      const wrapper = mount(RepositoryPackagesPublic, {
        attachTo: host,
        props: {
          repositoryId: "repo-paged",
          repositoryType: "python",
          repositoryKind: "proxy",
        },
        global: {
          stubs: vuetifyStubs,
        },
      });

      await flushPromises();
      await wrapper.vm.$nextTick();

      const firstPageHeaders = wrapper.findAll(".packages__table th");
      expect(firstPageHeaders.length).toBeGreaterThan(0);
      expect(wrapper.findAll(".packages__table th .column-resizer")).toHaveLength(
        firstPageHeaders.length,
      );

      const nextButton = wrapper
        .findAll('button[data-stub="v-btn"]')
        .find((button) => button.text().trim() === "Next");
      expect(nextButton).toBeTruthy();

      await nextButton!.trigger("click");
      await flushPromises();
      await wrapper.vm.$nextTick();

      const secondPageHeaders = wrapper.findAll(".packages__table th");
      expect(secondPageHeaders.length).toBeGreaterThan(0);
      expect(wrapper.findAll(".packages__table th .column-resizer")).toHaveLength(
        secondPageHeaders.length,
      );
      wrapper.unmount();
    } finally {
      host.remove();
      vi.useRealTimers();
    }
  });

  it("shows blob digest for Helm proxied packages", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "acme",
          name: "1.2.3",
          size: 4096,
          cache_path: "charts/acme-1.2.3.tgz",
          modified: "2025-11-05T09:30:00Z",
          blob_digest: "sha256:deadbeef",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-helm",
        repositoryType: "helm",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find('th[data-column="digest"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("Blob Digest");
    expect(wrapper.get('[data-testid="package-row"]').text()).toContain("sha256:deadbeef");
  });

  it("shows blob digest column for non-Helm repositories", async () => {
    (http.get as vi.Mock).mockResolvedValue(
      createPackages([
        {
          package: "pkg-alpha",
          name: "Alpha",
          size: 1024,
          cache_path: "cache/pkg-alpha",
          modified: "2025-11-05T09:30:00Z",
        },
      ]),
    );

    const wrapper = mount(RepositoryPackagesPublic, {
      props: {
        repositoryId: "repo-python",
        repositoryType: "python",
        repositoryKind: "hosted",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find('th[data-column="digest"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("Blob Digest");
  });
});
