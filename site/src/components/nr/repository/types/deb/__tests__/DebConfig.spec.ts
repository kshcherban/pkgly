import { flushPromises, mount } from "@vue/test-utils";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    put: vi.fn(),
    post: vi.fn(),
  },
}));

import http from "@/http";

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

let DebConfig: any;

beforeAll(async () => {
  DebConfig = (await import("../DebConfig.vue")).default;
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

const VComboboxStub = defineComponent({
  props: ["modelValue", "label"],
  emits: ["update:modelValue"],
  template: `<div class="combobox-stub">{{ label }}</div>`,
});

const SubmitButtonStub = defineComponent({
  props: ["type", "disabled", "loading"],
  emits: ["click"],
  template: `<button :type="type ?? 'submit'" :disabled="disabled" @click="$emit('click', $event)"><slot /></button>`,
});

const VSwitchStub = defineComponent({
  props: ["modelValue"],
  emits: ["update:modelValue"],
  template: `<input type="checkbox" :checked="modelValue" @change="$emit('update:modelValue', !modelValue)" />`,
});

const VDividerStub = defineComponent({ template: `<div class="divider-stub" />` });
const VAlertStub = defineComponent({ template: `<div class="alert-stub"><slot /></div>` });
const VTextFieldStub = defineComponent({
  props: ["modelValue"],
  emits: ["update:modelValue"],
  setup(_, { emit }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { onInput };
  },
  template: `<input :value="modelValue" @input="onInput" />`,
});

describe("DebConfig hosted/proxy switching", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("toggles refresh enabled for proxy configs", async () => {
    const Host = defineComponent({
      components: { DebConfig },
      setup() {
        const model = ref<any>({
          type: "proxy",
          config: {
            upstream_url: "https://deb.debian.org/debian",
            layout: {
              type: "dists",
              config: {
                distributions: ["stable"],
                components: ["main"],
                architectures: ["amd64", "all"],
              },
            },
            refresh: { enabled: false, schedule: { type: "interval_seconds", config: { interval_seconds: 3600 } } },
          },
        });
        return { model };
      },
      template: `<DebConfig v-model="model" />`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: SubmitButtonStub,
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-combobox": VComboboxStub,
          "v-switch": VSwitchStub,
          "v-divider": VDividerStub,
          "v-alert": VAlertStub,
          "v-text-field": VTextFieldStub,
        },
      },
    });

    await flushPromises();
    expect((wrapper.vm as any).model.config.refresh.enabled).toBe(false);

    const checkbox = wrapper.find("input[type='checkbox']");
    expect(checkbox.exists()).toBe(true);
    await checkbox.trigger("change");
    await flushPromises();

    expect((wrapper.vm as any).model.config.refresh.enabled).toBe(true);
  });

  it("switching to Proxy produces tagged proxy config payload", async () => {
    const Host = defineComponent({
      components: { DebConfig },
      setup() {
        const model = ref<any>({
          distributions: ["stable"],
          components: ["main"],
          architectures: ["amd64", "all"],
        });
        return { model };
      },
      template: `<DebConfig v-model="model" />`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: SubmitButtonStub,
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-combobox": VComboboxStub,
          "v-switch": VSwitchStub,
          "v-divider": VDividerStub,
          "v-alert": VAlertStub,
          "v-text-field": VTextFieldStub,
        },
      },
    });

    await flushPromises();
    const dropdowns = wrapper.findAllComponents(DropDownStub);
    expect(dropdowns.length).toBeGreaterThanOrEqual(1);
    await dropdowns[0]!.find("select").setValue("Proxy");
    await flushPromises();

    const model = (wrapper.vm as any).model;
    expect(model.type).toBe("proxy");
    expect(model.config.upstream_url).toBeTruthy();
    expect(model.config.layout.type).toBe("dists");
  });

  it("switching proxy layout to flat updates tagged layout payload", async () => {
    const Host = defineComponent({
      components: { DebConfig },
      setup() {
        const model = ref<any>({
          type: "proxy",
          config: {
            upstream_url: "https://deb.debian.org/debian",
            layout: {
              type: "dists",
              config: {
                distributions: ["stable"],
                components: ["main"],
                architectures: ["amd64", "all"],
              },
            },
          },
        });
        return { model };
      },
      template: `<DebConfig v-model="model" />`,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: SubmitButtonStub,
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-combobox": VComboboxStub,
          "v-switch": VSwitchStub,
          "v-divider": VDividerStub,
          "v-alert": VAlertStub,
          "v-text-field": VTextFieldStub,
        },
      },
    });

    await flushPromises();
    const dropdowns = wrapper.findAllComponents(DropDownStub);
    expect(dropdowns.length).toBeGreaterThanOrEqual(2);
    await dropdowns[1]!.find("select").setValue("flat");
    await flushPromises();

    const model = (wrapper.vm as any).model;
    expect(model.type).toBe("proxy");
    expect(model.config.layout.type).toBe("flat");
    expect(model.config.layout.config.distribution).toBeTruthy();
  });
});

describe("DebConfig manual refresh", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("renders Refresh Mirror button for proxy repos and posts to refresh endpoint", async () => {
    (http.get as vi.Mock)
      .mockResolvedValueOnce({
        data: {
          type: "proxy",
          config: {
            upstream_url: "https://nginx.org/packages/mainline/debian",
            layout: {
              type: "dists",
              config: { distributions: ["trixie"], components: ["nginx"], architectures: ["amd64"] },
            },
            refresh: { enabled: true, schedule: { type: "interval_seconds", config: { interval_seconds: 3600 } } },
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          in_progress: false,
          last_started_at: null,
          last_finished_at: null,
          last_success_at: null,
          last_error: null,
          last_downloaded_packages: null,
          last_downloaded_files: null,
          due: false,
          next_run_at: null,
        },
      })
      .mockResolvedValueOnce({
        data: {
          in_progress: true,
          last_started_at: "2025-12-13T00:00:00Z",
          last_finished_at: null,
          last_success_at: null,
          last_error: null,
          last_downloaded_packages: null,
          last_downloaded_files: null,
          due: false,
          next_run_at: null,
        },
      });

    (http.post as vi.Mock).mockResolvedValueOnce({ data: null });

    const wrapper = mount(DebConfig, {
      props: {
        repository: "repo-123",
      },
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: SubmitButtonStub,
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-combobox": VComboboxStub,
          "v-switch": VSwitchStub,
          "v-divider": VDividerStub,
          "v-alert": VAlertStub,
          "v-text-field": VTextFieldStub,
        },
      },
    });

    await flushPromises();

    const refreshButton = wrapper.find("[data-testid='deb-refresh-mirror']");
    expect(refreshButton.exists()).toBe(true);

    await refreshButton.trigger("click");
    await flushPromises();

    expect(http.post).toHaveBeenCalledWith("/api/repository/repo-123/deb/refresh");
  });
});

describe("DebConfig edit mode without v-model", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("allows toggling refresh enabled and persists via save", async () => {
    (http.get as vi.Mock)
      .mockResolvedValueOnce({
        data: {
          type: "proxy",
          config: {
            upstream_url: "https://nginx.org/packages/mainline/debian",
            layout: {
              type: "dists",
              config: { distributions: ["trixie"], components: ["nginx"], architectures: ["amd64"] },
            },
            // Simulate old repo config that predates refresh config.
            refresh: null,
          },
        },
      })
      .mockResolvedValueOnce({
        data: {
          in_progress: false,
          last_started_at: null,
          last_finished_at: null,
          last_success_at: null,
          last_error: null,
          last_downloaded_packages: null,
          last_downloaded_files: null,
          due: false,
          next_run_at: null,
        },
      })
      .mockResolvedValueOnce({
        data: {
          in_progress: false,
          last_started_at: null,
          last_finished_at: null,
          last_success_at: null,
          last_error: null,
          last_downloaded_packages: null,
          last_downloaded_files: null,
          due: false,
          next_run_at: null,
        },
      });

    (http.put as vi.Mock).mockResolvedValueOnce({ data: null });

    const wrapper = mount(DebConfig, {
      props: {
        repository: "repo-123",
      },
      global: {
        stubs: {
          DropDown: DropDownStub,
          TextInput: TextInputStub,
          SubmitButton: SubmitButtonStub,
          ProxyCacheNotice: defineComponent({ template: "<div />" }),
          "v-combobox": VComboboxStub,
          "v-switch": VSwitchStub,
          "v-divider": VDividerStub,
          "v-alert": VAlertStub,
          "v-text-field": VTextFieldStub,
        },
      },
    });

    await flushPromises();

    expect(http.put).not.toHaveBeenCalled();

    const checkbox = wrapper.find("input[type='checkbox']");
    expect(checkbox.exists()).toBe(true);
    await checkbox.trigger("change");
    await flushPromises();

    expect((wrapper.find("input[type='checkbox']").element as HTMLInputElement).checked).toBe(true);

    expect(http.put).toHaveBeenCalledWith("/api/repository/repo-123/config/deb", expect.anything());
    const [, payload] = (http.put as vi.Mock).mock.calls[0] ?? [];
    expect(payload?.config?.refresh?.enabled).toBe(true);
  });
});
