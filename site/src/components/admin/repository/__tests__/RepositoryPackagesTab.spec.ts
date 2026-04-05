import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick } from "vue";
import RepositoryPackagesTab from "@/components/admin/repository/RepositoryPackagesTab.vue";
import http from "@/http";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn().mockResolvedValue({
      data: {
        total_packages: 1,
        items: [
          {
            name: "express",
            size: 1024,
            cache_path: "/pkg/express-1.0.0.tgz",
            modified: "2024-01-01T00:00:00Z",
            package: "express",
          },
        ],
      },
      headers: {},
    }),
    delete: vi.fn(),
  },
}));

const mockAlerts = {
  success: vi.fn(),
  error: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

vi.mock("@/composables/useResizableColumns", () => ({
  useResizableColumns: () => ({
    initResizable: vi.fn(),
    cleanupResizers: vi.fn(),
  }),
}));

const VCardStub = defineComponent({
  template: "<div class='v-card'><slot /></div>",
});

const VCardTitleStub = defineComponent({
  template: "<div class='v-card-title'><slot /></div>",
});

const VCardSubtitleStub = defineComponent({
  template: "<div class='v-card-subtitle'><slot /></div>",
});

const VCardTextStub = defineComponent({
  template: "<div class='v-card-text'><slot /></div>",
});

const VCardActionsStub = defineComponent({
  template: "<div class='v-card-actions'><slot /></div>",
});

const VSpacerStub = defineComponent({
  template: "<span class='v-spacer' />",
});

const VTextFieldStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    clearable: {
      type: Boolean,
      default: false,
    },
  },
  emits: ["update:modelValue", "click:clear"],
  template: `
    <label class="v-text-field">
      <input
        :value="modelValue"
        @input="$emit('update:modelValue', $event.target.value)" />
      <button
        type="button"
        class="v-text-field__clear"
        @click="$emit('click:clear')">
        clear
      </button>
      <slot />
    </label>
  `,
});

const VBtnStub = defineComponent({
  emits: ["click"],
  template: "<button class='v-btn' @click=\"$emit('click')\"><slot /></button>",
});

const VDataTableStub = defineComponent({
  props: {
    headers: Array,
    items: Array,
    itemsLength: Number,
    itemsPerPage: [Number, String],
    hideDefaultFooter: Boolean,
    loading: Boolean,
  },
  template: "<table class='v-data-table'><slot /></table>",
});

const VProgressCircularStub = defineComponent({
  template: "<div class='v-progress-circular'><slot /></div>",
});

const VIconStub = defineComponent({
  template: "<i class='v-icon'><slot /></i>",
});

const VAlertStub = defineComponent({
  template: "<div class='v-alert'><slot /></div>",
});

const VCodeStub = defineComponent({
  template: "<code class='v-code'><slot /></code>",
});

const VPaginationStub = defineComponent({
  props: {
    modelValue: Number,
    length: Number,
  },
  emits: ["update:modelValue"],
  template: "<div class='v-pagination'><slot /></div>",
});

const VSelectStub = defineComponent({
  props: {
    modelValue: [String, Number],
    items: Array,
  },
  emits: ["update:modelValue"],
  template: "<select class='v-select'><slot /></select>",
});

const vuetifyStubs = {
  "v-card": VCardStub,
  "v-card-title": VCardTitleStub,
  "v-card-subtitle": VCardSubtitleStub,
  "v-card-text": VCardTextStub,
  "v-card-actions": VCardActionsStub,
  "v-spacer": VSpacerStub,
  "v-text-field": VTextFieldStub,
  "v-btn": VBtnStub,
  "v-data-table-server": VDataTableStub,
  "v-progress-circular": VProgressCircularStub,
  "v-icon": VIconStub,
  "v-alert": VAlertStub,
  "v-code": VCodeStub,
  "v-pagination": VPaginationStub,
  "v-select": VSelectStub,
};

describe("RepositoryPackagesTab.vue", () => {
  it("offers 500 and 1000 items per page", async () => {
    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const select = wrapper.getComponent(VSelectStub);
    const items = select.props("items") as unknown as number[] | undefined;
    expect(items).toBeDefined();
    expect(items).toContain(500);
    expect(items).toContain(1000);
  });

  it("marks search field clearable and clears search term", async () => {
    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const field = wrapper.getComponent(VTextFieldStub);
    expect(field.props("clearable")).toBe(true);

    field.vm.$emit("update:modelValue", "express");
    await nextTick();
    expect(wrapper.vm.searchTerm).toBe("express");

    await wrapper.get(".v-text-field__clear").trigger("click");
    await flushPromises();
    expect(wrapper.vm.searchTerm).toBe("");
  });

  it("reloads packages when items per page changes", async () => {
    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const httpGet = http.get as vi.Mock;
    httpGet.mockClear();

    (wrapper.vm as any).currentPage = 2;
    (wrapper.vm as any).handleItemsPerPageChange(100);

    await flushPromises();

    expect(httpGet).toHaveBeenCalledTimes(1);
    expect(httpGet).toHaveBeenCalledWith("/api/repository/1/packages", {
      params: { page: 1, per_page: 100, sort_by: "modified", sort_dir: "desc" },
    });
  });

  it("renders indexing warning headers", async () => {
    const httpGet = http.get as vi.Mock;
    httpGet.mockResolvedValueOnce({
      data: { total_packages: 0, items: [] },
      headers: { "x-pkgly-warning": "Repository indexing in progress" },
    });
    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const warning = wrapper.find('[data-testid="packages-indexing-warning"]');
    expect(warning.exists()).toBe(true);
    expect(warning.text()).toContain("Repository indexing in progress");
  });

  it("suppresses indexing warning headers for Ruby proxy repositories", async () => {
    const httpGet = http.get as vi.Mock;
    httpGet.mockResolvedValueOnce({
      data: { total_packages: 0, items: [] },
      headers: { "x-pkgly-warning": "Repository awaiting indexing" },
    });

    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "ruby",
        repositoryKind: "proxy",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const warning = wrapper.find('[data-testid="packages-indexing-warning"]');
    expect(warning.exists()).toBe(false);
  });

  it("requests server-side search across all pages", async () => {
    const httpGet = http.get as vi.Mock;
    httpGet.mockClear();

    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    httpGet.mockClear();

    const field = wrapper.getComponent(VTextFieldStub);
    field.vm.$emit("update:modelValue", "lodash");

    await flushPromises();

    expect(httpGet).toHaveBeenCalledTimes(1);
    expect(httpGet).toHaveBeenCalledWith("/api/repository/1/packages", {
      params: { page: 1, per_page: 50, sort_by: "modified", sort_dir: "desc", q: "lodash" },
    });
  });

  it("keeps search field visible when search returns zero results", async () => {
    const httpGet = http.get as vi.Mock;
    httpGet.mockClear();
    httpGet
      .mockResolvedValueOnce({
        data: {
          total_packages: 1,
          items: [
            {
              name: "express",
              size: 1024,
              cache_path: "/pkg/express-1.0.0.tgz",
              modified: "2024-01-01T00:00:00Z",
              package: "express",
            },
          ],
        },
        headers: {},
      })
      .mockResolvedValueOnce({
        data: {
          total_packages: 0,
          items: [],
        },
        headers: {},
      });

    const wrapper = mount(RepositoryPackagesTab, {
      props: {
        repositoryId: "1",
        repositoryType: "npm",
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const field = wrapper.getComponent(VTextFieldStub);
    field.vm.$emit("update:modelValue", "missing-package");
    await flushPromises();

    expect(wrapper.find(".v-text-field").exists()).toBe(true);
    expect(wrapper.vm.searchTerm).toBe("missing-package");
  });

  it("labels name column as Version for PHP proxy repositories", async () => {
    const wrapper = mount(RepositoryPackagesTab, {
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

    const headers = (wrapper.vm as any).headers as Array<{ key: string; title: string }>;
    const nameHeader = headers.find((header) => header.key === "name");
    expect(nameHeader?.title).toBe("Version");
  });

  it("adds blob digest column for Helm package listings", async () => {
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: {
        total_packages: 1,
        items: [
          {
            name: "1.2.3",
            size: 2048,
            cache_path: "charts/acme-1.2.3.tgz",
            modified: "2025-11-05T09:30:00Z",
            package: "acme",
            blob_digest: "sha256:deadbeef",
          },
        ],
      },
      headers: {},
    });

    const wrapper = mount(RepositoryPackagesTab, {
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

    const headers = wrapper.vm.headers as any[];
    expect(headers.some((header) => header.key === "blobDigest")).toBe(true);

    const tableItems = wrapper.vm.tableItems as any[];
    expect(tableItems[0]?.blobDigest).toBe("sha256:deadbeef");
  });

  it("adds blob digest column for non-Helm listings", async () => {
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: {
        total_packages: 1,
        items: [
          {
            name: "1.0.0",
            size: 128,
            cache_path: "packages/example/example-1.0.0.whl",
            modified: "2025-11-05T09:30:00Z",
            package: "example",
          },
        ],
      },
      headers: {},
    });

    const wrapper = mount(RepositoryPackagesTab, {
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

    const headers = wrapper.vm.headers as any[];
    expect(headers.some((header) => header.key === "blobDigest")).toBe(true);
  });
});
