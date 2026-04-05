import { flushPromises, mount } from "@vue/test-utils";
import { ref, defineComponent, h } from "vue";
import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    put: vi.fn(),
  },
}));

import MavenConfig from "../MavenConfig.vue";
import type { MavenConfigType } from "../maven";
import http from "@/http";

const dropDownStub = defineComponent({
  name: "DropDownStub",
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    options: {
      type: Array,
      default: () => [],
    },
  },
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    return () =>
      h(
        "select",
        {
          "data-stub": "dropdown",
          value: props.modelValue,
          onChange: (event: Event) =>
            emit("update:modelValue", (event.target as HTMLSelectElement).value),
        },
        [
          ...((props.options as Array<{ value: string; label: string }>) ?? []).map((option) =>
            h("option", { value: option.value }, option.label),
          ),
        ],
        slots.default?.(),
      );
  },
});

const textInputStub = defineComponent({
  name: "TextInputStub",
  props: {
    modelValue: {
      type: [String, Number],
      default: "",
    },
    disabled: {
      type: Boolean,
      default: false,
    },
  },
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    return () =>
      h(
        "input",
        {
          "data-stub": "text-input",
          value: props.modelValue as string | number,
          disabled: props.disabled,
          onInput: (event: Event) =>
            emit("update:modelValue", (event.target as HTMLInputElement).value),
        },
        slots.default?.(),
      );
  },
});

const submitButtonStub = defineComponent({
  name: "SubmitButtonStub",
  setup(_, { slots }) {
    return () =>
      h(
        "button",
        {
          type: "submit",
          "data-stub": "submit-button",
        },
        slots.default?.(),
      );
  },
});

const proxyConfigStub = defineComponent({
  name: "MavenProxyConfigStub",
  props: {
    modelValue: {
      type: Object,
      default: () => ({ routes: [] }),
    },
  },
  emits: ["update:modelValue"],
  setup(props, { slots }) {
    // Just pass through the model value without modification
    return () => h("div", { "data-stub": "proxy-config", "data-config": JSON.stringify(props.modelValue) }, slots.default?.());
  },
});

describe("MavenConfig.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("allows selecting proxy type during repository creation", async () => {
    const model = ref<MavenConfigType>({ type: "Hosted" });

    const wrapper = mount(MavenConfig, {
      props: {
        modelValue: model.value,
        "onUpdate:modelValue": (val: MavenConfigType) => {
          model.value = val;
        },
      },
      global: {
        stubs: {
          DropDown: dropDownStub,
          TextInput: textInputStub,
          SubmitButton: submitButtonStub,
          MavenProxyConfig: proxyConfigStub,
          VCard: { template: "<div data-stub='v-card'><slot /></div>" },
          VCardText: { template: "<div data-stub='v-card-text'><slot /></div>" },
          VRow: { template: "<div data-stub='v-row'><slot /></div>" },
          VCol: { template: "<div data-stub='v-col'><slot /></div>" },
          VExpandTransition: { template: "<div data-stub='v-expand-transition'><slot /></div>" },
        },
      },
    });

    expect(wrapper.find('[data-testid="maven-config-card"]').exists()).toBe(true);
    await wrapper.get('[data-stub="dropdown"]').setValue("Proxy");
    expect(model.value.type).toBe("Proxy");
  });

  it("loads and saves existing repository configuration", async () => {
    const loadedConfig = { type: "Proxy" as const, config: { routes: [] } };
    (http.get as vi.Mock).mockResolvedValue({
      data: loadedConfig,
    });
    (http.put as vi.Mock).mockResolvedValue(undefined);

    const model = ref<MavenConfigType>({ type: "Hosted" });

    const wrapper = mount(MavenConfig, {
      props: {
        repository: "repo-1",
        modelValue: model.value,
        "onUpdate:modelValue": async (val: MavenConfigType) => {
          model.value = val;
          await wrapper.setProps({ modelValue: val });
        },
      },
      global: {
        stubs: {
          DropDown: dropDownStub,
          TextInput: textInputStub,
          SubmitButton: submitButtonStub,
          MavenProxyConfig: proxyConfigStub,
          VCard: { template: "<div data-stub='v-card'><slot /></div>" },
          VCardText: { template: "<div data-stub='v-card-text'><slot /></div>" },
          VCardActions: { template: "<div data-stub='v-card-actions'><slot /></div>" },
          VRow: { template: "<div data-stub='v-row'><slot /></div>" },
          VCol: { template: "<div data-stub='v-col'><slot /></div>" },
          VDivider: { template: "<hr data-stub='v-divider' />" },
          VExpandTransition: { template: "<div data-stub='v-expand-transition'><slot /></div>" },
        },
      },
    });

    await flushPromises();
    expect(model.value.type).toBe("Proxy");
    expect(model.value).toEqual(loadedConfig);

    await wrapper.find("form").trigger("submit");
    await flushPromises();
    expect(http.put).toHaveBeenCalledWith("/api/repository/repo-1/config/maven", loadedConfig);
  });
});
