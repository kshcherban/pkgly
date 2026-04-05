import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent, ref } from "vue";

import SwitchInput from "../SwitchInput.vue";

const VSwitchStub = defineComponent({
  props: ["id", "modelValue"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onChange = () => {
      emit("update:modelValue", !props.modelValue);
    };
    return { props, slots, onChange };
  },
  template: `
    <div class="v-switch-stub">
      <input :id="id" type="checkbox" :checked="modelValue" @change="onChange" />
      <slot name="label" />
    </div>
  `,
});

describe("SwitchInput.vue", () => {
  it("toggles when the underlying switch toggles", async () => {
    const Host = defineComponent({
      components: { SwitchInput },
      setup() {
        const enabled = ref(false);
        return { enabled };
      },
      template: `<SwitchInput id="switch-test" v-model="enabled">Enable</SwitchInput>`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
    });

    expect((wrapper.vm as any).enabled).toBe(false);
    const checkbox = wrapper.find("input#switch-test");
    expect(checkbox.exists()).toBe(true);
    await checkbox.trigger("change");
    expect((wrapper.vm as any).enabled).toBe(true);
  });
});
