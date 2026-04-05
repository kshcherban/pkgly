import { flushPromises, mount } from "@vue/test-utils";
import { beforeAll, describe, expect, it } from "vitest";
import { defineComponent, ref } from "vue";

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

let RubyConfig: any;

beforeAll(async () => {
  RubyConfig = (await import("../RubyConfig.vue")).default;
});

const DropDownStub = defineComponent({
  props: ["modelValue", "options", "disabled"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onChange = (event: Event) => {
      const next = (event.target as HTMLSelectElement).value;
      emit("update:modelValue", next);
    };
    return { props, slots, onChange };
  },
  template: `
    <label class="dropdown-stub">
      <slot />
      <select :disabled="disabled" :value="modelValue" @change="onChange">
        <option v-for="option in options" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </select>
    </label>
  `,
});

const TextInputStub = defineComponent({
  props: ["modelValue", "placeholder", "id"],
  emits: ["update:modelValue"],
  setup(_, { emit, slots }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { slots, onInput };
  },
  template: `
    <label class="text-input-stub">
      <slot />
      <input :id="id" :placeholder="placeholder" :value="modelValue" @input="onInput" />
    </label>
  `,
});

describe("RubyConfig hosted/proxy switching", () => {
  it("switching to Proxy produces tagged proxy config payload", async () => {
    const Host = defineComponent({
      components: { RubyConfig },
      setup() {
        const model = ref<any>({ type: "Hosted" });
        return { model };
      },
      template: `<RubyConfig v-model="model" />`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: defineComponent({ template: "<button type='submit'><slot /></button>" }),
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
        },
      },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(DropDownStub);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const model = (wrapper.vm as any).model;
    expect(model.type).toBe("Proxy");
    expect(model.config.upstream_url).toBe("https://rubygems.org");
  });

  it("parses revalidation TTL as an integer and supports clearing", async () => {
    const Host = defineComponent({
      components: { RubyConfig },
      setup() {
        const model = ref<any>({ type: "Hosted" });
        return { model };
      },
      template: `<RubyConfig v-model="model" />`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: defineComponent({ template: "<button type='submit'><slot /></button>" }),
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
        },
      },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(DropDownStub);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const ttlInput = wrapper.find("input[placeholder='300']");
    expect(ttlInput.exists()).toBe(true);
    await ttlInput.setValue("123.9");
    await flushPromises();

    expect((wrapper.vm as any).model.config.revalidation_ttl_seconds).toBe(123);

    await ttlInput.setValue("");
    await flushPromises();
    expect((wrapper.vm as any).model.config.revalidation_ttl_seconds).toBeUndefined();
  });
});

