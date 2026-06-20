// ABOUTME: Verifies admin repository operations, table hierarchy, and empty states.
// ABOUTME: Covers real request state transitions through focused component stubs.
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import RepositoryListView from "@/views/admin/repository/RepositoryListView.vue";
import type { RepositoryWithStorageName } from "@/types/repository";
import http from "@/http";

const pushMock = vi.fn();
const mockGetStorages = vi.fn();

vi.mock("vue-router", () => ({
  useRouter: () => ({
    push: pushMock,
  }),
}));

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getStorages: mockGetStorages,
  }),
}));

const repositories: RepositoryWithStorageName[] = [
  {
    id: "repo-1",
    storage_name: "primary",
    storage_id: "storage-1",
    name: "alpha",
    repository_type: "npm",
    repository_kind: null,
    active: true,
    visibility: "Private",
    updated_at: "2025-11-09T12:00:00Z",
    created_at: "2025-10-01T09:15:00Z",
    auth_enabled: true,
    storage_usage_bytes: 1_048_576,
    storage_usage_updated_at: "2025-11-10T08:00:00Z",
  },
];

const httpGet = http.get as unknown as vi.Mock;

const stubs = {
  "v-container": defineComponent({
    template: "<div data-testid='container'><slot /></div>",
  }),
  "v-row": defineComponent({
    template: "<div class='v-row'><slot /></div>",
  }),
  "v-col": defineComponent({
    template: "<div class='v-col'><slot /></div>",
  }),
  "v-card": defineComponent({
    emits: ["click"],
    template: "<div class='v-card'><slot /></div>",
  }),
  "v-card-text": defineComponent({
    template: "<div class='v-card-text'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-alert": defineComponent({
    template: "<div data-testid='repository-error'><slot /></div>",
  }),
  "v-btn": defineComponent({
    props: {
      loading: Boolean,
      disabled: Boolean,
      color: String,
      variant: String,
      prependIcon: String,
      icon: String,
      to: [String, Object],
    },
    emits: ["click"],
    template: `
      <button
        type="button"
        :data-to-name="to && typeof to === 'object' ? to.name : to"
        @click="$emit('click')">
        <slot />{{ ariaLabel }}
      </button>
    `,
    computed: {
      ariaLabel() {
        return this.$attrs["aria-label"] ?? "";
      },
    },
  }),
  "v-progress-circular": defineComponent({
    template: "<div data-testid='loading-indicator'><slot /></div>",
  }),
  "v-chip": defineComponent({
    props: {
      color: String,
      variant: String,
      size: String,
    },
    template: "<span class='v-chip'><slot /></span>",
  }),
  "v-spacer": defineComponent({
    template: "<span data-testid='spacer'></span>",
  }),
  "v-icon": defineComponent({
    props: {
      icon: String,
      color: String,
      size: [String, Number],
    },
    template: "<i class='v-icon'><slot /></i>",
  }),
  "v-data-table": defineComponent({
    props: {
      items: {
        type: Array,
        default: () => [],
      },
      headers: {
        type: Array,
        default: () => [],
      },
      loading: {
        type: Boolean,
        default: false,
      },
      itemValue: {
        type: String,
        default: "id",
      },
    },
    emits: ["click:row"],
    template: `
      <div data-testid="repository-table">
        <div
          v-for="item in items"
          :key="item.id"
          data-testid="repository-row"
          @click="$emit('click:row', {}, { item })">
          {{ item.name }}
        </div>
        <slot name="no-data" />
      </div>
    `,
  }),
};

describe("RepositoryListView.vue", () => {
  beforeEach(() => {
    pushMock.mockReset();
    httpGet.mockReset();
    mockGetStorages.mockReset();
    mockGetStorages.mockResolvedValue([
      { id: "storage-1", name: "primary", storage_type: "local", active: true },
    ]);
  });

  it("marks every visible column sortable so the client-side table can sort", async () => {
    httpGet.mockResolvedValue({ data: repositories });

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();

    const headers = wrapper.getComponent(stubs["v-data-table"]).props("headers") as Array<{
      title: string;
      sortable: boolean;
      key: string;
    }>;
    for (const header of headers) {
      expect(header.sortable, `${header.title} should be sortable`).toBe(true);
    }
    // Access sorts by the auth_label value (Secured/Unsecured).
    const access = headers.find((header) => header.key === "access");
    expect(access?.sortable).toBe(true);
    expect(access?.value).toBe("auth_label");
  });

  it("renders the repository table once data is loaded", async () => {
    httpGet.mockResolvedValue({ data: repositories });

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();

    const table = wrapper.find("[data-testid='repository-table']");
    expect(table.exists()).toBe(true);
    expect(wrapper.findAll("[data-testid='repository-row']")).toHaveLength(repositories.length);
    expect(wrapper.findAll("button").filter((button) => button.text().includes("Create Repository"))).toHaveLength(1);

    const headers = wrapper.getComponent(stubs["v-data-table"]).props("headers") as Array<{
      title: string;
    }>;
    expect(headers.map((header) => header.title)).toEqual([
      "Repository",
      "Storage",
      "Access",
      "Usage",
      "Usage Updated",
    ]);
    expect(headers.map((header) => header.title)).not.toContain("ID #");
  });

  it("maps repository metadata and statuses to consistent labels", async () => {
    httpGet.mockResolvedValue({
      data: [
        repositories[0],
        {
          ...repositories[0],
          id: "repo-2",
          repository_kind: "proxy",
          auth_enabled: false,
          active: false,
          storage_usage_bytes: null,
          storage_usage_updated_at: "not-a-date",
        },
        {
          ...repositories[0],
          id: "repo-3",
          repository_kind: "virtual",
        },
      ],
    });

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });
    await flushPromises();

    const items = (wrapper.vm as any).tableItems;
    expect(items.map((item: any) => item.repository_kind)).toEqual([
      "Hosted",
      "Proxy",
      "Virtual",
    ]);
    expect(items[0].auth_label).toBe("Secured");
    expect(items[0].active_label).toBe("Active");
    expect(items[1].auth_label).toBe("Unsecured");
    expect(items[1].active_label).toBe("Inactive");
    expect((wrapper.vm as any).formatBytes(null)).toBe("Not available");
    expect((wrapper.vm as any).formatUpdatedAt(null)).toBe("Not available");
    expect((wrapper.vm as any).formatUpdatedAt("not-a-date")).toBe("Not available");
  });

  it("navigates to the repository details when a row is clicked", async () => {
    httpGet.mockResolvedValue({ data: repositories });

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();
    await wrapper.get("[data-testid='repository-row']").trigger("click");

    expect(pushMock).toHaveBeenCalledWith({
      name: "AdminViewRepository",
      params: { id: "repo-1" },
    });
  });

  it("displays an error message when the repository request fails", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    httpGet.mockRejectedValue(new Error("boom"));

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();

    expect(wrapper.find("[data-testid='repository-error']").text()).toContain("Failed to fetch repositories");
    consoleError.mockRestore();
  });

  it("preserves loaded rows while refreshing usage", async () => {
    let resolveRefresh: ((value: unknown) => void) | undefined;
    httpGet
      .mockResolvedValueOnce({ data: repositories })
      .mockReturnValueOnce(new Promise((resolve) => {
        resolveRefresh = resolve;
      }));

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });
    await flushPromises();

    const refresh = wrapper
      .findAll("button")
      .find((button) => button.text().includes("Refresh"));
    await refresh!.trigger("click");

    expect(wrapper.findAll("[data-testid='repository-row']")).toHaveLength(1);
    expect(wrapper.getComponent(stubs["v-data-table"]).props("loading")).toBe(true);
    expect(httpGet).toHaveBeenLastCalledWith("/api/repository/list", {
      params: {
        include_usage: true,
        refresh_usage: true,
      },
    });

    resolveRefresh?.({ data: repositories });
    await flushPromises();
    expect(wrapper.getComponent(stubs["v-data-table"]).props("loading")).toBe(false);
  });

  it("shows one create repository CTA when no repositories exist but storage exists", async () => {
    httpGet.mockResolvedValue({ data: [] });

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();

    const createRepositoryButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().includes("Create Repository"));
    expect(wrapper.text()).toContain("No repositories found");
    expect(wrapper.text()).toContain("Create your first repository to get started.");
    expect(createRepositoryButtons).toHaveLength(1);
    expect(createRepositoryButtons[0]!.attributes("data-to-name")).toBe("RepositoryCreate");
  });

  it("directs users to create storage when repository creation has no storage target", async () => {
    httpGet.mockResolvedValue({ data: [] });
    mockGetStorages.mockResolvedValue([]);

    const wrapper = mount(RepositoryListView, {
      global: { stubs },
    });

    await flushPromises();

    const createStorageButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().includes("Create Storage"));
    expect(wrapper.text()).toContain("No storages found");
    expect(wrapper.text()).toContain("Create a storage before adding repositories.");
    expect(wrapper.text()).not.toContain("Create Repository");
    expect(createStorageButtons).toHaveLength(1);
    expect(createStorageButtons[0]!.attributes("data-to-name")).toBe("StorageCreate");
  });
});
