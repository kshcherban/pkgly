import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import SubmitButton from "@/components/form/SubmitButton.vue";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/http", () => ({
  default: {
    post: vi.fn(),
  },
}));

const mockAlerts = {
  success: vi.fn(),
  error: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

const controlStubs = {
  TextInput: defineComponent({
    props: ["modelValue", "id"],
    emits: ["update:modelValue"],
    template: `
      <label class="text-input">
        <slot />
        <input
          :id="id"
          :value="modelValue"
          @input="$emit('update:modelValue', $event.target.value)" />
      </label>
    `,
  }),
  RepositoryToActionList: defineComponent({
    props: ["modelValue"],
    emits: ["update:modelValue"],
    setup(props, { emit }) {
      const value = ref(props.modelValue ?? []);
      const add = () => {
        value.value = [...value.value, { repositoryId: "repo-1", actions: { asArray: () => [] } }];
        emit("update:modelValue", value.value);
      };
      return { value, add };
    },
    template: `
      <div data-testid="repository-scope-stub">
        <button type="button" @click="add">Add</button>
      </div>
    `,
  }),
  ScopesSelector: defineComponent({
    props: ["modelValue"],
    emits: ["update:modelValue"],
    template: `<div data-testid="scopes-selector-stub"></div>`,
  }),
  CopyCode: defineComponent({
    props: ["code"],
    template: "<pre data-testid='token-output'>{{ code }}</pre>",
  }),
};

const vuetifyStubs = {
  "v-container": defineComponent({
    template: "<div data-testid='token-create-container'><slot /></div>",
  }),
  "v-card": defineComponent({
    template: "<div data-testid='token-create-card'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-card-text": defineComponent({
    template: "<div class='v-card-text'><slot /></div>",
  }),
  "v-form": defineComponent({
    emits: ["submit"],
    template: "<form data-testid='token-create-form'><slot /></form>",
  }),
  "v-row": defineComponent({
    template: "<div class='v-row'><slot /></div>",
  }),
  "v-col": defineComponent({
    props: { cols: [Number, String], md: [Number, String] },
    template: "<div class='v-col'><slot /></div>",
  }),
  "v-alert": defineComponent({
    template: "<div data-testid='token-create-alert'><slot /></div>",
  }),
  "v-divider": defineComponent({
    template: "<hr class='v-divider' />",
  }),
  "v-progress-circular": defineComponent({
    template: "<div data-testid='token-create-loading'></div>",
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: { color: String, variant: String, type: { type: String, default: "button" } },
    emits: ["click"],
    setup(props, { attrs, emit, slots }) {
      return () =>
        h(
          "button",
          {
            class: ["v-btn", attrs.class],
            type: props.type,
            onClick: (event: MouseEvent) => emit("click", event),
          },
          slots.default ? slots.default() : undefined,
        );
    },
  }),
};

describe("TokenCreate.vue", () => {
  let TokenCreate: any;

  class LocalStorageMock implements Storage {
    private store = new Map<string, string>();

    get length(): number {
      return this.store.size;
    }

    clear(): void {
      this.store.clear();
    }

    getItem(key: string): string | null {
      return this.store.get(key) ?? null;
    }

    key(index: number): string | null {
      return Array.from(this.store.keys())[index] ?? null;
    }

    removeItem(key: string): void {
      this.store.delete(key);
    }

    setItem(key: string, value: string): void {
      this.store.set(key, value);
    }
  }

  const mockLocalStorage = new LocalStorageMock();

  beforeEach(async () => {
    mockAlerts.success.mockReset();
    mockAlerts.error.mockReset();
    Object.defineProperty(globalThis, "localStorage", {
      value: mockLocalStorage,
      configurable: true,
    });
    Object.defineProperty(window, "localStorage", {
      value: mockLocalStorage,
      configurable: true,
    });

    const module = await import("@/views/profile/TokenCreate.vue");
    TokenCreate = module.default;
  });

  afterEach(() => {
    vi.resetModules();
  });

  it("renders the new form layout with a fixed-width create button", async () => {
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await flushPromises();

    expect(wrapper.find('[data-testid="token-create-card"]').exists()).toBe(true);
    expect(wrapper.find('.token-create__actions').exists()).toBe(true);
    const submitWrapper = wrapper.findComponent(SubmitButton);
    expect(submitWrapper.exists()).toBe(true);
    expect(submitWrapper.props("block")).toBe(false);

    const buttonEl = wrapper.get(".v-btn");
    expect(buttonEl.text()).toContain("Create Token");
    expect(buttonEl.classes()).toContain("submit-button");
    expect(buttonEl.classes()).toContain("submit-button--fixed");
  });
});
