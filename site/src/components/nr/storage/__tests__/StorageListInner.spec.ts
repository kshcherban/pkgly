import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import StorageListInner from "@/components/nr/storage/StorageListInner.vue";

vi.mock("@/router", () => ({
  default: {
    push: vi.fn(),
  },
}));

describe("StorageListInner.vue", () => {
  it("provides a clear control for the storage search input", async () => {
    const wrapper = mount(StorageListInner, {
      props: {
        storages: [
          {
            id: 1,
            name: "Primary",
            storage_type: "s3",
            active: true,
          },
        ],
      },
    });

    const input = wrapper.get("input#nameSearch");
    await input.setValue("Prim");
    expect(wrapper.vm.searchValue).toBe("Prim");

    const clearButton = wrapper.find("[data-testid='storage-search-clear']");
    expect(clearButton.exists()).toBe(true);

    await clearButton.trigger("click");
    expect(wrapper.vm.searchValue).toBe("");
  });
});
