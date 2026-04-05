import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import SubmitButton from "@/components/form/SubmitButton.vue";

const vuetifyStubs = {
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: {
      type: {
        type: String,
        default: "button",
      },
      color: String,
      variant: String,
      block: Boolean,
      loading: Boolean,
      disabled: Boolean,
    },
    emits: ["click"],
    setup(props, { slots, emit, attrs }) {
      return () =>
        h(
          "button",
          {
            class: ["v-btn", attrs.class],
            type: props.type,
            disabled: props.disabled,
            onClick: (event: MouseEvent) => emit("click", event),
          },
          slots.default ? slots.default() : undefined,
        );
    },
  }),
  "v-icon": defineComponent({
    inheritAttrs: false,
    setup(_props, { slots }) {
      return () => h("i", { class: "v-icon" }, slots.default ? slots.default() : undefined);
    },
  }),
};

describe("SubmitButton.vue", () => {
  it("renders as a Vuetify button with submit semantics", () => {
    const wrapper = mount(SubmitButton, {
      slots: { default: "Create" },
      global: {
        stubs: vuetifyStubs,
      },
    });

    const button = wrapper.get("button");
    expect(button.classes()).toContain("v-btn");
    expect(button.classes()).toContain("submit-button");
    expect(button.classes()).toContain("submit-button--fixed");
    expect(button.attributes("type")).toBe("submit");
    expect(button.text()).toBe("Create");
  });

  it("emits a click event when activated", async () => {
    const wrapper = mount(SubmitButton, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await wrapper.get("button").trigger("click");

    expect(wrapper.emitted("click")).toHaveLength(1);
  });

  it("reacts to disabled prop changes", async () => {
    const wrapper = mount(SubmitButton, {
      props: { disabled: true },
      global: {
        stubs: vuetifyStubs,
      },
    });

    const button = wrapper.get("button");
    expect(button.attributes("disabled")).toBeDefined();

    await wrapper.setProps({ disabled: false });

    expect(button.attributes("disabled")).toBeUndefined();
  });
});
