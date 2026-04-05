import { flushPromises, mount } from "@vue/test-utils";
import { beforeAll, describe, expect, it } from "vitest";
import { defineComponent, h } from "vue";

const storageMock = {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
  clear: () => undefined,
};

(globalThis as any).localStorage = storageMock;
if (typeof window !== "undefined") {
  (window as any).localStorage = storageMock;
}

let GoConfig: any;

beforeAll(async () => {
  GoConfig = (await import("../GoConfig.vue")).default;
});

const DropDownStub = defineComponent({
  props: ["modelValue", "options", "required", "disabled"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onChange = (event: Event) => {
      const target = event.target as HTMLSelectElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { props, slots, onChange };
  },
  template: `
    <label class="dropdown-stub">
      <slot />
      <select
        :disabled="disabled"
        :required="required"
        :value="modelValue"
        @change="onChange">
        <option v-for="option in options" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </select>
    </label>
  `,
});

const NumberInputStub = defineComponent({
  props: ["modelValue", "min", "max", "placeholder", "error"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", Number(target?.value ?? 0));
    };
    return { props, slots, onInput };
  },
  template: `
    <label class="number-input-stub">
      <slot />
      <input
        type="number"
        :min="min"
        :max="max"
        :placeholder="placeholder"
        :value="modelValue"
        @input="onInput" />
      <span class="error" v-if="error">{{ error }}</span>
    </label>
  `,
});

const TextInputStub = defineComponent({
  props: ["modelValue", "placeholder", "error"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { props, slots, onInput };
  },
  template: `
    <label class="text-input-stub">
      <slot />
      <input
        :placeholder="placeholder"
        :value="modelValue"
        @input="onInput" />
      <span class="error" v-if="error">{{ error }}</span>
    </label>
  `,
});

const VBtnStub = defineComponent({
  inheritAttrs: false,
  emits: ["click"],
  setup(_, { emit, attrs, slots }) {
    return () =>
      h(
        "button",
        {
          ...attrs,
          type: (attrs.type as string) || "button",
          disabled: attrs.disabled as boolean | undefined,
          onClick: () => emit("click"),
        },
        slots.default?.(),
      );
  },
});

describe("GoConfig proxy route layout", () => {
  it("gives remove buttons the route action class for full-width alignment", async () => {
    const wrapper = mount(GoConfig, {
      props: { settingName: "go" },
      global: {
        stubs: {
          DropDown: DropDownStub,
          NumberInput: NumberInputStub,
          TextInput: TextInputStub,
          SubmitButton: defineComponent({ template: "<button type='submit'><slot /></button>" }),
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-btn": VBtnStub,
        },
      },
    });

    await flushPromises();
    const removeBtn = wrapper.get(".route-row button");
    expect(removeBtn.classes()).toContain("route-action");
  });

  it("renders Add Route button below routes matching npm layout", async () => {
    const wrapper = mount(GoConfig, {
      props: { settingName: "go" },
      global: {
        stubs: {
          DropDown: DropDownStub,
          NumberInput: NumberInputStub,
          TextInput: TextInputStub,
          SubmitButton: defineComponent({ template: "<button type='submit'><slot /></button>" }),
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-btn": VBtnStub,
        },
      },
    });

    await flushPromises();
    const addBtn = wrapper.get(".route-add");
    expect(addBtn.text()).toContain("Add Route");
  });
});
