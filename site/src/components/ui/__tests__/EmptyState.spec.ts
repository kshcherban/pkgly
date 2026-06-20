// ABOUTME: Tests EmptyState renders icon, title, message, and optional action slot.
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent } from "vue";
import EmptyState from "@/components/ui/EmptyState.vue";

const VIconStub = defineComponent({
  name: "VIcon",
  props: { icon: String, size: [String, Number], color: String },
  template: '<i class="v-icon" :data-icon="icon"><slot /></i>',
});

function mountEmpty(props: Record<string, unknown>, slots: Record<string, any> = {}) {
  return mount(EmptyState, {
    props,
    slots,
    global: { stubs: { "v-icon": VIconStub } },
  });
}

describe("EmptyState", () => {
  it("renders the icon, title, and message", () => {
    const wrapper = mountEmpty({
      icon: "mdi-package-variant",
      title: "No repositories available",
      message: "Contact your administrator.",
    });

    expect(wrapper.find(".v-icon").attributes("data-icon")).toBe("mdi-package-variant");
    expect(wrapper.text()).toContain("No repositories available");
    expect(wrapper.text()).toContain("Contact your administrator.");
  });

  it("renders without a message when none is provided", () => {
    const wrapper = mountEmpty({ icon: "mdi-magnify", title: "Nothing here" });
    expect(wrapper.text()).toContain("Nothing here");
    expect(wrapper.find(".empty-state__message").exists()).toBe(false);
  });

  it("renders an action from the slot when provided", () => {
    const Action = defineComponent({
      template: "<button data-testid='cta'>Create Repository</button>",
    });
    const wrapper = mountEmpty(
      { icon: "mdi-package-variant", title: "Empty" },
      { action: Action },
    );

    expect(wrapper.find('[data-testid="cta"]').exists()).toBe(true);
  });

  it("exposes a region landmark for screen readers", () => {
    const wrapper = mountEmpty({ icon: "mdi-package", title: "Empty" });
    expect(wrapper.find("[role='status']").exists()).toBe(true);
  });
});
