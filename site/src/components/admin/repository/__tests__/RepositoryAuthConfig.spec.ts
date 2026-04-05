import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn().mockResolvedValue({ data: { enabled: false } }),
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
      <slot />
      <slot name="comment" />
    </label>
  `,
});

const VCardStub = defineComponent({
  template: `<div class="v-card-stub"><slot /></div>`,
});

const VCardTextStub = defineComponent({
  template: `<div class="v-card-text-stub"><slot /></div>`,
});

const VAlertStub = defineComponent({
  props: {
    type: {
      type: String,
      default: "info",
    },
  },
  template: `<div class="v-alert-stub" :data-type="type" data-testid="auth-alert"><slot /></div>`,
});

const RepositoryAuthConfig = (await import("@/components/admin/repository/configs/RepositoryAuthConfig.vue")).default;
const http = (await import("@/http")).default;

function mountHarness() {
  const Harness = defineComponent({
    components: { RepositoryAuthConfig },
    setup() {
      const state = ref({ enabled: false });
      return { state };
    },
    template: `<RepositoryAuthConfig v-model="state" repository="repo-1" />`,
  });

  return mount(Harness, {
    global: {
      stubs: {
        SwitchInput: SwitchInputStub,
        "v-card": VCardStub,
        "v-card-text": VCardTextStub,
        "v-alert": VAlertStub,
      },
    },
  });
}

function mountCreateHarness() {
  const Harness = defineComponent({
    components: { RepositoryAuthConfig },
    template: `<RepositoryAuthConfig />`,
  });

  return mount(Harness, {
    global: {
      stubs: {
        SwitchInput: SwitchInputStub,
        "v-card": VCardStub,
        "v-card-text": VCardTextStub,
        "v-alert": VAlertStub,
      },
    },
  });
}

describe("RepositoryAuthConfig", () => {
  it("enables authentication by default for new repositories", async () => {
    const wrapper = mountCreateHarness();
    await flushPromises();

    const toggle = wrapper.getComponent(SwitchInputStub);
    expect(toggle.props("modelValue")).toBe(true);
  });

  it("does not show success message before user interacts", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    expect(wrapper.find('[data-testid="auth-alert"]').exists()).toBe(false);
    expect(http.put).not.toHaveBeenCalled();
  });

  it("shows success message after toggling", async () => {
    const wrapper = mountHarness();
    await flushPromises();

    await wrapper.find(".switch-input-stub__button").trigger("click");
    await flushPromises();

    const alert = wrapper.find('[data-testid="auth-alert"]').text();
    expect(alert).toContain("Authentication settings saved.");
  });
});
