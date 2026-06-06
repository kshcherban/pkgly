// ABOUTME: Verifies copy-code accessibility and clipboard behavior.
// ABOUTME: Covers the reusable copy control used by repository setup.
import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import CopyCode from "@/components/core/code/CopyCode.vue";

const success = vi.fn();

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => ({
    success,
  }),
}));

describe("CopyCode.vue", () => {
  it("copies code from an explicitly named button", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", {
      clipboard: { writeText },
    });
    const wrapper = mount(CopyCode, {
      props: {
        code: "https://pkgly.test/repositories/main/npm",
        label: "Repository URL",
      },
      slots: {
        default: "Repository URL",
      },
      global: {
        stubs: {
          "v-icon": { template: "<i><slot /></i>" },
        },
      },
    });

    await wrapper.get('button[aria-label="Copy Repository URL"]').trigger("click");

    expect(writeText).toHaveBeenCalledWith(
      "https://pkgly.test/repositories/main/npm",
    );
    expect(success).toHaveBeenCalledWith("Copied");
  });
});
