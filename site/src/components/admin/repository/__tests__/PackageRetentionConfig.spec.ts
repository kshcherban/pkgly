// ABOUTME: Tests manual-save behavior for the package retention repository config UI.
// ABOUTME: Covers draft input, validation, save, and cancel interactions.
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn().mockResolvedValue({
      data: {
        enabled: false,
        max_age_days: 30,
        keep_latest_per_package: 1,
      },
    }),
    put: vi.fn().mockResolvedValue({}),
  },
}));

vi.mock("@vue/devtools-kit", () => ({}));

const SwitchInputStub = defineComponent({
  props: {
    modelValue: {
      type: Boolean,
      default: false,
    },
    disabled: {
      type: Boolean,
      default: false,
    },
  },
  emits: ["update:modelValue"],
  template: `
    <label class="switch-input-stub">
      <button
        type="button"
        class="switch-input-stub__button"
        :disabled="disabled"
        @click="$emit('update:modelValue', !modelValue)">
        Toggle
      </button>
      <span class="switch-input-stub__value">{{ String(modelValue) }}</span>
      <slot />
    </label>
  `,
});

const SubmitButtonStub = defineComponent({
  props: {
    disabled: Boolean,
    loading: Boolean,
    block: {
      type: Boolean,
      default: true,
    },
  },
  emits: ["click"],
  template: `
    <button
      type="button"
      data-testid="retention-save"
      :disabled="disabled"
      @click="$emit('click', $event)">
      <slot />
    </button>
  `,
});

const VBtnStub = defineComponent({
  props: {
    disabled: Boolean,
  },
  emits: ["click"],
  template: `
    <button
      type="button"
      data-testid="retention-cancel"
      :disabled="disabled"
      @click="$emit('click', $event)">
      <slot />
    </button>
  `,
});

const VTextFieldStub = defineComponent({
  props: {
    modelValue: {
      type: [String, Number],
      default: "",
    },
    id: String,
    disabled: Boolean,
    errorMessages: {
      type: [String, Array],
      default: "",
    },
  },
  emits: ["update:modelValue"],
  template: `
    <label class="v-text-field-stub">
      <input
        :id="id"
        :value="modelValue"
        :disabled="disabled"
        @input="$emit('update:modelValue', $event.target.value)" />
      <span
        v-if="Array.isArray(errorMessages) ? errorMessages.length : errorMessages"
        class="v-text-field-stub__error">
        {{ Array.isArray(errorMessages) ? errorMessages.join(", ") : errorMessages }}
      </span>
    </label>
  `,
});

const VCardStub = defineComponent({
  template: `<div class="v-card-stub"><slot /></div>`,
});

const VCardTextStub = defineComponent({
  template: `<div class="v-card-text-stub"><slot /></div>`,
});

const VCardActionsStub = defineComponent({
  template: `<div class="v-card-actions-stub"><slot /></div>`,
});

const VAlertStub = defineComponent({
  props: {
    type: {
      type: String,
      default: "info",
    },
  },
  template: `<div class="v-alert-stub" :data-type="type" data-testid="retention-alert"><slot /></div>`,
});

const passthroughStub = defineComponent({
  template: `<div><slot /></div>`,
});

const PackageRetentionConfig = (await import("@/components/admin/repository/configs/PackageRetentionConfig.vue")).default;
const http = (await import("@/http")).default;

function mountHarness() {
  const Harness = defineComponent({
    components: { PackageRetentionConfig },
    setup() {
      const state = ref({
        enabled: false,
        max_age_days: 30,
        keep_latest_per_package: 1,
      });
      return { state };
    },
    template: `<PackageRetentionConfig v-model="state" repository="repo-1" />`,
  });

  return mount(Harness, {
    global: {
      stubs: {
        SwitchInput: SwitchInputStub,
        SubmitButton: SubmitButtonStub,
        "v-btn": VBtnStub,
        "v-text-field": VTextFieldStub,
        "v-card": VCardStub,
        "v-card-text": VCardTextStub,
        "v-card-actions": VCardActionsStub,
        "v-alert": VAlertStub,
        "v-row": passthroughStub,
        "v-col": passthroughStub,
        "v-divider": passthroughStub,
      },
    },
  });
}

function mountCreateHarness() {
  const Harness = defineComponent({
    components: { PackageRetentionConfig },
    setup() {
      const state = ref({
        enabled: false,
        max_age_days: 30,
        keep_latest_per_package: 1,
      });
      return { state };
    },
    template: `<PackageRetentionConfig v-model="state" />`,
  });

  return mount(Harness, {
    global: {
      stubs: {
        SwitchInput: SwitchInputStub,
        SubmitButton: SubmitButtonStub,
        "v-btn": VBtnStub,
        "v-text-field": VTextFieldStub,
        "v-card": VCardStub,
        "v-card-text": VCardTextStub,
        "v-card-actions": VCardActionsStub,
        "v-alert": VAlertStub,
        "v-row": passthroughStub,
        "v-col": passthroughStub,
        "v-divider": passthroughStub,
      },
    },
  });
}

describe("PackageRetentionConfig", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(http.get).mockResolvedValue({
      data: {
        enabled: false,
        max_age_days: 30,
        keep_latest_per_package: 1,
      },
    });
    vi.mocked(http.put).mockResolvedValue({});
  });

  it("loads retention settings without autosaving", async () => {
    mountHarness();
    await flushPromises();

    expect(http.get).toHaveBeenCalledWith(
      "/api/repository/repo-1/config/package_retention",
      {
        params: { default: true },
      },
    );
    expect(http.put).not.toHaveBeenCalled();
  });

  it("does not save while the user edits draft fields", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    await wrapper.get("#package-retention-max-age").setValue("90");
    await wrapper.find(".switch-input-stub__button").trigger("click");
    await flushPromises();

    expect(http.put).not.toHaveBeenCalled();
  });

  it("blocks save and shows validation when a number is invalid", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    await wrapper.get("#package-retention-max-age").setValue("");
    await flushPromises();

    expect(wrapper.text()).toContain("Enter a whole number greater than or equal to 1.");
    expect(wrapper.get('[data-testid="package-retention-save"]').attributes("disabled")).toBeDefined();

    await wrapper.get('[data-testid="package-retention-save"]').trigger("click");
    await flushPromises();

    expect(http.put).not.toHaveBeenCalled();
  });

  it("saves all changed fields in one request", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    await wrapper.find(".switch-input-stub__button").trigger("click");
    await wrapper.get("#package-retention-max-age").setValue("90");
    await wrapper.get("#package-retention-keep-latest").setValue("3");
    await wrapper.get('[data-testid="package-retention-save"]').trigger("click");
    await flushPromises();

    expect(http.put).toHaveBeenCalledTimes(1);
    expect(http.put).toHaveBeenCalledWith(
      "/api/repository/repo-1/config/package_retention",
      {
        enabled: true,
        max_age_days: 90,
        keep_latest_per_package: 3,
      },
    );
    expect(wrapper.text()).toContain("Retention settings saved.");
  });

  it("cancels draft changes without saving", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    await wrapper.get("#package-retention-max-age").setValue("90");
    await wrapper.get("#package-retention-keep-latest").setValue("5");
    await wrapper.get('[data-testid="package-retention-cancel"]').trigger("click");
    await flushPromises();

    expect((wrapper.get("#package-retention-max-age").element as HTMLInputElement).value).toBe("30");
    expect((wrapper.get("#package-retention-keep-latest").element as HTMLInputElement).value).toBe("1");
    expect(http.put).not.toHaveBeenCalled();
  });

  it("keeps create mode v-model updates without rendering save actions", async () => {
    const wrapper = mountCreateHarness();
    await flushPromises();

    await wrapper.get("#package-retention-max-age").setValue("45");
    await flushPromises();

    expect(wrapper.find('[data-testid="package-retention-save"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="package-retention-cancel"]').exists()).toBe(false);
    expect((wrapper.vm as any).state.max_age_days).toBe(45);
    expect(http.get).not.toHaveBeenCalled();
    expect(http.put).not.toHaveBeenCalled();
  });
});
