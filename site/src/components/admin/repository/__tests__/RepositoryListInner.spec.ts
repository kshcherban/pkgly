import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import RepositoryListInner from "@/components/admin/repository/RepositoryListInner.vue";

vi.mock("@/router", () => ({
  default: {
    push: vi.fn(),
  },
}));

describe("RepositoryListInner.vue", () => {
  it("shows a clear button that resets the search value", async () => {
    const wrapper = mount(RepositoryListInner, {
      props: {
        repositories: [
          {
            id: 1,
            name: "Alpha",
            storage_name: "Primary",
            repository_type: "npm",
            auth_enabled: true,
            storage_usage_bytes: 0,
            active: true,
            storage_usage_updated_at: "2024-01-01T00:00:00Z",
          },
        ],
      },
    });

    const input = wrapper.get("input#nameSearch");
    await input.setValue("Alpha");
    expect(wrapper.vm.searchValue).toBe("Alpha");

    const clearButton = wrapper.find("[data-testid='repository-search-clear']");
    expect(clearButton.exists()).toBe(true);

    await clearButton.trigger("click");
    expect(wrapper.vm.searchValue).toBe("");
  });
});
