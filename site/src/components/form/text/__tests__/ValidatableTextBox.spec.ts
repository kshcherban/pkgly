import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent } from "vue";
import { describe, expect, it } from "vitest";
import ValidatableTextBox from "@/components/form/text/ValidatableTextBox.vue";
import type { ValidationType } from "@/components/form/text/validations";

const VTextFieldStub = defineComponent({
  props: {
    id: String,
    type: String,
    modelValue: {
      type: String,
      default: "",
    },
    error: Boolean,
  },
  emits: ["update:modelValue", "focus", "blur", "keydown"],
  template: `
    <label>
      <input
        :id="id"
        :type="type"
        :value="modelValue"
        :data-error="error ? 'true' : 'false'"
        @input="$emit('update:modelValue', $event.target.value)"
        @focus="$emit('focus', $event)"
        @blur="$emit('blur', $event)"
        @keydown="$emit('keydown', $event)" />
    </label>
  `,
});

const InputRequirementsStub = defineComponent({
  props: {
    show: Boolean,
    validations: {
      type: Array,
      default: () => [],
    },
    results: {
      type: Object,
      default: () => ({}),
    },
  },
  template: `<div class="input-requirements-stub" />`,
});

function lastValidity(wrapper: ReturnType<typeof mount>) {
  const emitted = wrapper.emitted("validity");
  expect(emitted).toBeTruthy();
  return emitted![emitted!.length - 1][0];
}

describe("ValidatableTextBox.vue", () => {
  const validations: ValidationType[] = [
    {
      id: "matches-original",
      message: "Must match the original value.",
      validate: (value: string, originalValue?: string) => value === originalValue,
      isAsync: false,
    },
  ];

  it("keeps the original value valid on mount", async () => {
    const wrapper = mount(ValidatableTextBox, {
      props: {
        id: "email",
        modelValue: "test@example.com",
        originalValue: "test@example.com",
        validations,
      },
      global: {
        stubs: {
          "v-text-field": VTextFieldStub,
          InputRequirements: InputRequirementsStub,
        },
      },
    });

    await flushPromises();

    expect(wrapper.attributes("data-valid")).toBe("true");
    expect(lastValidity(wrapper)).toBe(true);
    expect(wrapper.get("input").attributes("data-error")).toBe("false");
  });

  it("marks changed values invalid when they no longer match the original", async () => {
    const wrapper = mount(ValidatableTextBox, {
      props: {
        id: "email",
        modelValue: "test@example.com",
        originalValue: "test@example.com",
        validations,
      },
      global: {
        stubs: {
          "v-text-field": VTextFieldStub,
          InputRequirements: InputRequirementsStub,
        },
      },
    });

    await flushPromises();
    await wrapper.get("input").setValue("other@example.com");
    await flushPromises();

    expect(wrapper.attributes("data-valid")).toBe("false");
    expect(lastValidity(wrapper)).toBe(false);
    expect(wrapper.get("input").attributes("data-error")).toBe("true");
  });
});
