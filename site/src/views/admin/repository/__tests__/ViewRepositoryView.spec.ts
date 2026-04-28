// ABOUTME: Tests the repository administration view tab layout and config wiring.
// ABOUTME: Uses component stubs to verify repository metadata and dynamic config props.
import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { defineComponent } from "vue";
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

const routerMock = {
  currentRoute: {
    value: {
      params: { id: "repo-123" },
    },
  },
  push: vi.fn(),
};

vi.mock("@/router", () => ({
  default: routerMock,
}));

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

vi.mock("@vue/devtools-kit", () => ({}));

const BasicRepositoryInfoStub = defineComponent({
  props: {
    repository: Object,
    embedded: Boolean,
  },
  template: `<div data-testid="basic-repo-info" :data-embedded="String(embedded)">{{ repository?.name }}</div>`,
});

const RepositoryPackagesTabStub = defineComponent({
  props: ["repositoryId", "repositoryType", "repositoryKind"],
  template: `<div data-testid="packages-tab"></div>`,
});

const DynamicConfigStub = defineComponent({
  props: ["repository"],
  template: `<div data-testid="config-stub">config for {{ repository }}</div>`,
});

const VContainerStub = defineComponent({
  template: `<div class="v-container"><slot /></div>`,
});
const VCardStub = defineComponent({
  template: `<div class="v-card"><slot /></div>`,
});
const VTabsStub = defineComponent({
  props: { modelValue: String },
  emits: ["update:modelValue"],
  template: `<div class="v-tabs"><slot /></div>`,
});
const VTabStub = defineComponent({
  props: { value: String },
  emits: ["click"],
  template: `<button class="v-tab" :data-value="value" @click="$emit('click')"><slot /></button>`,
});
const VDividerStub = defineComponent({
  template: `<div class="v-divider"></div>`,
});
const VWindowStub = defineComponent({
  props: { modelValue: String },
  emits: ["update:modelValue"],
  template: `<div class="v-window"><slot /></div>`,
});
const VWindowItemStub = defineComponent({
  props: { value: String },
  template: `<div class="v-window-item"><slot /></div>`,
});

const http = (await import("@/http")).default as { get: vi.Mock };

const repositoryResponse = {
  id: "repo-123",
  storage_name: "s3-store",
  storage_id: "storage-123",
  name: "docker-proxy",
  repository_type: "docker",
  repository_kind: "proxy",
  active: true,
  visibility: "Private",
  updated_at: "2025-11-20T10:00:00Z",
  created_at: "2025-11-19T10:00:00Z",
  auth_enabled: true,
  storage_usage_bytes: null,
  storage_usage_updated_at: null,
};

const phpRepositoryResponse = {
  ...repositoryResponse,
  id: "repo-php",
  name: "composer-hosted",
  repository_type: "php",
  repository_kind: "hosted",
};

const rubyRepositoryResponse = {
  ...repositoryResponse,
  id: "repo-ruby",
  name: "rubygems-hosted",
  repository_type: "ruby",
  repository_kind: "hosted",
};

const s3StorageResponse = {
  id: "storage-123",
  name: "s3-store",
  storage_type: "s3",
  active: true,
  created_at: "2025-11-18T12:00:00Z",
  config: {
    type: "S3",
    settings: {
      bucket_name: "cache-bucket",
      region: "us-east-1",
      path_style: true,
      credentials: {
        access_key: "access",
        secret_key: "secret",
      },
      cache: {
        enabled: true,
        path: "/var/cache/pkgly",
        max_bytes: 1048576,
        max_entries: 32,
      },
    },
  },
};

function mockHttpSequence() {
  http.get.mockImplementation((url: string) => {
    if (url === "/api/repository/repo-123") {
      return Promise.resolve({ data: repositoryResponse });
    }
    if (url === "/api/repository/repo-123/configs") {
      return Promise.resolve({ data: ["docker"] });
    }
    if (url === "/api/repository/repo-123/config/docker") {
      return Promise.resolve({ data: { type: "Proxy", config: { upstream_url: "https://registry-1.docker.io" } } });
    }
    if (url === "/api/storage/storage-123") {
      return Promise.resolve({ data: s3StorageResponse });
    }
    return Promise.reject(new Error(`Unhandled URL ${url}`));
  });
}

function mockPhpHttpSequence() {
  http.get.mockImplementation((url: string) => {
    if (url === "/api/repository/repo-php") {
      return Promise.resolve({ data: phpRepositoryResponse });
    }
    if (url === "/api/repository/repo-php/configs") {
      return Promise.resolve({ data: ["php"] });
    }
    if (url === "/api/repository/repo-php/config/php") {
      return Promise.resolve({ data: { type: "Hosted" } });
    }
    if (url === "/api/storage/storage-123") {
      return Promise.resolve({ data: s3StorageResponse });
    }
    return Promise.reject(new Error(`Unhandled URL ${url}`));
  });
}

function mockRubyHttpSequence() {
  http.get.mockImplementation((url: string) => {
    if (url === "/api/repository/repo-ruby") {
      return Promise.resolve({ data: rubyRepositoryResponse });
    }
    if (url === "/api/repository/repo-ruby/configs") {
      return Promise.resolve({ data: ["ruby"] });
    }
    if (url === "/api/repository/repo-ruby/config/ruby") {
      return Promise.resolve({ data: { type: "Hosted" } });
    }
    if (url === "/api/storage/storage-123") {
      return Promise.resolve({ data: s3StorageResponse });
    }
    return Promise.reject(new Error(`Unhandled URL ${url}`));
  });
}

function mockRetentionHttpSequence() {
  http.get.mockImplementation((url: string) => {
    if (url === "/api/repository/repo-123") {
      return Promise.resolve({ data: repositoryResponse });
    }
    if (url === "/api/repository/repo-123/configs") {
      return Promise.resolve({ data: ["package_retention"] });
    }
    if (url === "/api/repository/repo-123/config/docker") {
      return Promise.resolve({ data: { type: "Proxy", config: { upstream_url: "https://registry-1.docker.io" } } });
    }
    if (url === "/api/repository/config/package_retention/description") {
      return Promise.resolve({ data: { name: "Package Retention", description: "Retention settings" } });
    }
    if (url === "/api/storage/storage-123") {
      return Promise.resolve({ data: s3StorageResponse });
    }
    return Promise.reject(new Error(`Unhandled URL ${url}`));
  });
}

describe("ViewRepositoryView", () => {
  let pinia: Pinia;

  beforeEach(() => {
    pinia = createPinia();
    setActivePinia(pinia);
    http.get.mockReset();
    routerMock.currentRoute.value.params.id = "repo-123";
  });

  it("shows S3 cache settings for the repository storage", async () => {
    mockHttpSequence();
    const ViewRepositoryView = (await import("@/views/admin/repository/ViewRepositoryView.vue")).default;

    const wrapper = mount(ViewRepositoryView, {
      global: {
        plugins: [pinia],
        stubs: {
          BasicRepositoryInfo: BasicRepositoryInfoStub,
          RepositoryPackagesTab: RepositoryPackagesTabStub,
          FallBackEditor: DynamicConfigStub,
          DockerConfig: DynamicConfigStub,
          "v-container": VContainerStub,
          "v-card": VCardStub,
          "v-tabs": VTabsStub,
          "v-tab": VTabStub,
          "v-divider": VDividerStub,
          "v-window": VWindowStub,
          "v-window-item": VWindowItemStub,
        },
      },
    });

    await flushPromises();

    // Storage tab should be present
    const tabs = wrapper.findAll(".v-tab");
    const storageTab = tabs.find((tab) => tab.attributes("data-value") === "storage");
    expect(storageTab, "Storage tab missing").toBeDefined();

    // Cache details rendered
    const cacheText = wrapper.text();
    expect(cacheText).toContain("/var/cache/pkgly");
    expect(cacheText).toContain("1.00 MB");
    expect(cacheText).toContain("32 entries");
  });

  it("embeds repository info without rendering a nested card surface", async () => {
    mockHttpSequence();
    const ViewRepositoryView = (await import("@/views/admin/repository/ViewRepositoryView.vue")).default;

    const wrapper = mount(ViewRepositoryView, {
      global: {
        plugins: [pinia],
        stubs: {
          BasicRepositoryInfo: BasicRepositoryInfoStub,
          RepositoryPackagesTab: RepositoryPackagesTabStub,
          FallBackEditor: DynamicConfigStub,
          DockerConfig: DynamicConfigStub,
          "v-container": VContainerStub,
          "v-card": VCardStub,
          "v-tabs": VTabsStub,
          "v-tab": VTabStub,
          "v-divider": VDividerStub,
          "v-window": VWindowStub,
          "v-window-item": VWindowItemStub,
        },
      },
    });

    await flushPromises();

    expect(wrapper.get('[data-testid="basic-repo-info"]').attributes("data-embedded")).toBe("true");
  });

  it("shows packages tab for PHP repositories", async () => {
    routerMock.currentRoute.value.params.id = "repo-php";
    mockPhpHttpSequence();
    const ViewRepositoryView = (await import("@/views/admin/repository/ViewRepositoryView.vue")).default;

    const wrapper = mount(ViewRepositoryView, {
      global: {
        plugins: [pinia],
        stubs: {
          BasicRepositoryInfo: BasicRepositoryInfoStub,
          RepositoryPackagesTab: RepositoryPackagesTabStub,
          FallBackEditor: DynamicConfigStub,
          PhpConfig: DynamicConfigStub,
          "v-container": VContainerStub,
          "v-card": VCardStub,
          "v-tabs": VTabsStub,
          "v-tab": VTabStub,
          "v-divider": VDividerStub,
          "v-window": VWindowStub,
          "v-window-item": VWindowItemStub,
        },
      },
    });

    await flushPromises();

    const tabs = wrapper.findAll(".v-tab");
    const packagesTab = tabs.find((tab) => tab.attributes("data-value") === "packages");
    expect(packagesTab, "Packages tab should be visible for PHP repositories").toBeDefined();
    expect(wrapper.find("[data-testid='packages-tab']").exists()).toBe(true);
  });

  it("shows packages tab for Ruby repositories", async () => {
    routerMock.currentRoute.value.params.id = "repo-ruby";
    mockRubyHttpSequence();
    const ViewRepositoryView = (await import("@/views/admin/repository/ViewRepositoryView.vue")).default;

    const wrapper = mount(ViewRepositoryView, {
      global: {
        plugins: [pinia],
        stubs: {
          BasicRepositoryInfo: BasicRepositoryInfoStub,
          RepositoryPackagesTab: RepositoryPackagesTabStub,
          FallBackEditor: DynamicConfigStub,
          RubyConfig: DynamicConfigStub,
          "v-container": VContainerStub,
          "v-card": VCardStub,
          "v-tabs": VTabsStub,
          "v-tab": VTabStub,
          "v-divider": VDividerStub,
          "v-window": VWindowStub,
          "v-window-item": VWindowItemStub,
        },
      },
    });

    await flushPromises();

    const tabs = wrapper.findAll(".v-tab");
    const packagesTab = tabs.find((tab) => tab.attributes("data-value") === "packages");
    expect(packagesTab, "Packages tab should be visible for Ruby repositories").toBeDefined();
    expect(wrapper.find("[data-testid='packages-tab']").exists()).toBe(true);
  });

  it("renders package retention with the repository id", async () => {
    mockRetentionHttpSequence();
    const ViewRepositoryView = (await import("@/views/admin/repository/ViewRepositoryView.vue")).default;

    const wrapper = mount(ViewRepositoryView, {
      global: {
        plugins: [pinia],
        stubs: {
          BasicRepositoryInfo: BasicRepositoryInfoStub,
          RepositoryPackagesTab: RepositoryPackagesTabStub,
          FallBackEditor: DynamicConfigStub,
          PackageRetentionConfig: DynamicConfigStub,
          "v-container": VContainerStub,
          "v-card": VCardStub,
          "v-tabs": VTabsStub,
          "v-tab": VTabStub,
          "v-divider": VDividerStub,
          "v-window": VWindowStub,
          "v-window-item": VWindowItemStub,
        },
      },
    });

    await flushPromises();

    const tabs = wrapper.findAll(".v-tab");
    const retentionTab = tabs.find((tab) => tab.attributes("data-value") === "package_retention");
    expect(retentionTab, "Package Retention tab should be visible").toBeDefined();
    expect(wrapper.find("[data-testid='config-stub']").text()).toContain("config for repo-123");
  });
});
