import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent } from "vue";
import JsonSchemaForm from "@/components/form/JsonSchemaForm.vue";

const booleanField = {
  key: () => "allow_tag_overwrite",
  title: () => "Allow Tag Overwrite",
  type: () => "boolean",
  default: () => false,
};

const stubForm = {
  getProperties: () => [booleanField],
};

describe("JsonSchemaForm", () => {
  it("emits updated model when boolean toggle changes", async () => {
    const VSwitchStub = defineComponent({
      props: {
        modelValue: Boolean,
      },
      emits: ["update:modelValue"],
      template: `
        <label class="v-switch-stub">
          <input
            type="checkbox"
            :checked="modelValue"
            @change="$emit('update:modelValue', $event.target.checked)" />
        </label>
      `,
    });

    const wrapper = mount(JsonSchemaForm, {
      props: {
        form: stubForm as any,
        modelValue: { allow_tag_overwrite: false },
      },
      global: {
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
    });

    await flushPromises();

    const checkbox = wrapper.find("input[type='checkbox']");
    expect(checkbox.exists()).toBe(true);
    await checkbox.setValue(true);

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toBeTruthy();
    expect(emitted?.[0]?.[0]).toMatchObject({ allow_tag_overwrite: true });
  });
});
