import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { describe, expect, it, vi, beforeEach } from "vitest";
vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    put: vi.fn(),
  },
}));

import HelmConfig from "@/components/nr/repository/types/helm/HelmConfig.vue";
import http from "@/http";

const modelStub = defineComponent({
  name: "FormFieldStub",
  props: {
    modelValue: {
      type: [String, Number, Boolean, Object],
      default: undefined,
    },
    options: {
      type: Array,
      default: () => [],
    },
    placeholder: {
      type: String,
      default: "",
    },
    inputmode: {
      type: String,
      default: undefined,
    },
    pattern: {
      type: String,
      default: undefined,
    },
    required: {
      type: Boolean,
      default: false,
    },
    id: {
      type: String,
      default: undefined,
    },
  },
  emits: ["update:modelValue"],
  setup(_, { slots }) {
    return () => h("div", { class: "form-field-stub" }, slots.default?.());
  },
});

describe("HelmConfig.vue", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("renders helm config form using themed controls and actions", async () => {
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: {
        overwrite: true,
        index_cache_ttl: 600,
        mode: "http",
        public_base_url: "https://charts.example.com/repo",
        max_chart_size: 10485760,
        max_file_count: 128,
      },
    });

    const wrapper = mount(HelmConfig, {
      props: {
        repository: "repo-123",
        settingName: "helm",
      },
      global: {
        stubs: {
          DropDown: modelStub,
          SwitchInput: modelStub,
          TextInput: modelStub,
          SubmitButton: defineComponent({
            name: "SubmitButton",
            setup(_, { slots }) {
              return () => h("button", { "data-testid": "submit-button-stub" }, slots.default?.());
            },
          }),
        },
      },
    });

    await flushPromises();

    expect(wrapper.find('[data-testid="helm-config-container"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="helm-config-save"]').exists()).toBe(true);
    expect(wrapper.findAll('[data-testid="helm-config-field"]').length).toBeGreaterThan(0);
  });

  it("saves the Helm config and refreshes values after persistence", async () => {
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: {
        overwrite: false,
        index_cache_ttl: 300,
        mode: "http",
        public_base_url: null,
        max_chart_size: null,
        max_file_count: null,
      },
    });
    (http.get as vi.Mock).mockResolvedValueOnce({
      data: {
        overwrite: true,
        index_cache_ttl: 600,
        mode: "oci",
        public_base_url: "https://charts.example.com/helm",
        max_chart_size: 1024,
        max_file_count: 10,
      },
    });
    (http.put as vi.Mock).mockResolvedValue(undefined);

    const wrapper = mount(HelmConfig, {
      props: {
        repository: "repo-123",
        settingName: "helm",
      },
      global: {
        stubs: {
          DropDown: modelStub,
          SwitchInput: modelStub,
          TextInput: modelStub,
          SubmitButton: defineComponent({
            name: "SubmitButton",
            setup(_, { slots }) {
              return () => h("button", {}, slots.default?.());
            },
          }),
        },
      },
    });

    await flushPromises();

    // Simulate toggling overwrite prior to saving
    wrapper.vm.value.overwrite = true;

    await wrapper.vm.save();
    await flushPromises();

    expect(http.put).toHaveBeenCalledWith(
      "/api/repository/repo-123/config/helm",
      expect.objectContaining({ overwrite: true })
    );
    expect((http.get as vi.Mock).mock.calls).toHaveLength(2);
    expect(wrapper.vm.value.overwrite).toBe(true);
    expect(wrapper.vm.value.index_cache_ttl).toBe(600);
  });
});
