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
      to: [String, Object],
    },
    emits: ["click"],
    template: `
      <button
        type="button"
        :data-to-name="to && typeof to === 'object' ? to.name : to"
        @click="$emit('click')">
        <slot />
      </button>
    `,
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
