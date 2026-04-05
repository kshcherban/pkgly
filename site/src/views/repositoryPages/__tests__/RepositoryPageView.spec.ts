import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import RepositoryPageView from "@/views/repositoryPages/RepositoryPageView.vue";

vi.mock("@/router", () => {
  const push = vi.fn();
  const currentRoute = ref({
    params: {
      repositoryId: "repo-1",
    },
  });
  return {
    default: {
      currentRoute,
      push,
    },
  };
});

const mockRepository = {
  id: "repo-1",
  name: "example",
  storage_name: "primary",
  storage_id: "storage-1",
  repository_type: "npm",
  repository_kind: null,
  active: true,
  visibility: "Public",
  updated_at: "2025-11-09T00:00:00Z",
  created_at: "2025-01-01T00:00:00Z",
  auth_enabled: false,
  storage_usage_bytes: null,
  storage_usage_updated_at: null,
};

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositoryById: vi.fn().mockResolvedValue(mockRepository),
    getRepositoryIdByNames: vi.fn(),
  }),
}));

const simpleStub = defineComponent({
  template: "<div><slot /></div>",
});

describe("RepositoryPageView.vue", () => {
  it("renders packages without requiring a custom page", async () => {
    const wrapper = mount(RepositoryPageView, {
      global: {
        stubs: {
          "v-container": simpleStub,
          "v-card": simpleStub,
          "v-card-text": simpleStub,
          CopyURL: simpleStub,
          RepositoryHelper: simpleStub,
          RepositoryIcon: simpleStub,
          RepositoryPackagesPublic: defineComponent({
            template: "<div data-testid='repository-packages'>Packages</div>",
          }),
        },
      },
    });

    await flushPromises();

    expect(wrapper.find('[data-testid="repository-packages"]').exists()).toBe(true);
  });

  it("keeps the header collapsed by default and expands it with helper content and icon-first metadata", async () => {
    const wrapper = mount(RepositoryPageView, {
      global: {
        stubs: {
          "v-container": simpleStub,
          "v-card": simpleStub,
          "v-card-text": simpleStub,
          CopyURL: defineComponent({
            template: "<div data-testid='copy-url'>Copy URL</div>",
          }),
          RepositoryHelper: defineComponent({
            template: "<div data-testid='repository-helper'>Repository helper</div>",
          }),
          RepositoryIcon: defineComponent({
            template: "<div data-testid='repository-icon'>Repo icon</div>",
          }),
          RepositoryPackagesPublic: defineComponent({
            template: "<div data-testid='repository-packages'>Packages</div>",
          }),
        },
      },
    });

    await flushPromises();

    expect(wrapper.find(".repository-page__header-details").exists()).toBe(false);
    expect(wrapper.find('[data-testid="repository-packages"]').exists()).toBe(true);

    await wrapper.get('[data-testid="repository-header-toggle"]').trigger("click");

    const header = wrapper.get(".repository-page__header");
    const meta = wrapper.get(".repository-page__meta").html();
    expect(header.text()).toContain("Repository helper");
    expect(meta.indexOf("repository-icon")).toBeLessThan(meta.indexOf("copy-url"));
  });
});
