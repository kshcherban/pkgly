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
    props: ["modelValue", "id", "disabled", "placeholder", "type", "min"],
    emits: ["update:modelValue"],
    template: `
      <label class="text-input">
        <slot />
        <input
          :id="id"
          :disabled="disabled"
          :placeholder="placeholder"
          :type="type"
          :min="min"
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
        value.value = [...value.value, { repositoryId: "repo-1", actions: { asArray: () => ["Read"] } }];
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
    setup(props, { emit }) {
      const add = () => emit("update:modelValue", [{ key: "ReadRepository", name: "Read Repository" }]);
      return { add };
    },
    template: `
      <div data-testid="scopes-selector-stub">
        <button type="button" data-testid="add-role-scope" @click="add">Add scope</button>
      </div>
    `,
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
    template: "<form data-testid='token-create-form' @submit.prevent=\"$emit('submit', $event)\"><slot /></form>",
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
  "v-icon": defineComponent({
    template: "<span class='v-icon'><slot /></span>",
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: {
      color: String,
      variant: String,
      type: { type: String, default: "button" },
      disabled: Boolean,
    },
    emits: ["click"],
    setup(props, { attrs, emit, slots }) {
      return () =>
        h(
          "button",
          {
            class: ["v-btn", attrs.class],
            ...attrs,
            type: props.type,
            disabled: props.disabled,
            onClick: (event: MouseEvent) => emit("click", event),
          },
          slots.default ? slots.default() : undefined,
        );
    },
  }),
};

describe("TokenCreate.vue", () => {
  let TokenCreate: any;
  const writeText = vi.fn();

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
    const http = await import("@/http");
    vi.mocked(http.default.post).mockReset();
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
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    writeText.mockReset();

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

    const buttonEl = wrapper.get("button.submit-button");
    expect(buttonEl.text()).toContain("Create Token");
    expect(buttonEl.classes()).toContain("submit-button");
    expect(buttonEl.classes()).toContain("submit-button--fixed");
  });

  it("renders expiration lifetime controls", async () => {
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await flushPromises();

    expect(wrapper.get('[data-testid="expiration-never"]').text()).toContain("Never");
    expect(wrapper.get('[data-testid="expiration-preset-7"]').text()).toContain("7 days");
    expect(wrapper.get('[data-testid="expiration-preset-30"]').text()).toContain("30 days");
    expect(wrapper.get('[data-testid="expiration-preset-90"]').text()).toContain("90 days");
    expect(wrapper.get("#customExpirationDays").exists()).toBe(true);
  });

  it("submits preset expiration days and backend repository scope shape", async () => {
    const http = await import("@/http");
    vi.mocked(http.default.post).mockResolvedValue({
      data: { id: 1, token: "pkgly_token", expires_at: "2026-05-02T00:00:00Z" },
    });
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await wrapper.get("#tokenName").setValue("CI");
    await wrapper.get('[data-testid="repository-scope-stub"] button').trigger("click");
    await wrapper.get('[data-testid="expiration-preset-30"]').trigger("click");
    await wrapper.get('[data-testid="token-create-form"]').trigger("submit.prevent");
    await flushPromises();

    expect(http.default.post).toHaveBeenCalledWith("/api/user/token/create", {
      name: "CI",
      description: "",
      expires_in_days: 30,
      repository_scopes: [{ repository_id: "repo-1", scopes: ["Read"] }],
      scopes: [],
    });
  });

  it("submits null expiration for never-expiring tokens", async () => {
    const http = await import("@/http");
    vi.mocked(http.default.post).mockResolvedValue({
      data: { id: 1, token: "pkgly_token", expires_at: null },
    });
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await wrapper.get('[data-testid="add-role-scope"]').trigger("click");
    await wrapper.get('[data-testid="expiration-never"]').trigger("click");
    await wrapper.get('[data-testid="token-create-form"]').trigger("submit.prevent");
    await flushPromises();

    expect(http.default.post).toHaveBeenCalled();
    expect(vi.mocked(http.default.post).mock.calls[0]?.[1]).toMatchObject({
      expires_in_days: null,
      scopes: ["ReadRepository"],
    });
  });

  it("blocks invalid custom expiration days before submit", async () => {
    const http = await import("@/http");
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await wrapper.get('[data-testid="expiration-custom"]').trigger("click");
    await wrapper.get("#customExpirationDays").setValue("0");
    await wrapper.get('[data-testid="token-create-form"]').trigger("submit.prevent");
    await flushPromises();

    expect(http.default.post).not.toHaveBeenCalled();
    expect(mockAlerts.error).toHaveBeenCalledWith(
      "Invalid expiration",
      "Custom expiration must be a positive whole number of days.",
    );
  });

  it("renders and copies the one-time token panel", async () => {
    const http = await import("@/http");
    vi.mocked(http.default.post).mockResolvedValue({
      data: { id: 1, token: "pkgly_secret_value", expires_at: "2026-05-02T00:00:00Z" },
    });
    const wrapper = mount(TokenCreate, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...controlStubs,
        },
      },
    });

    await wrapper.get('[data-testid="add-role-scope"]').trigger("click");
    await wrapper.get('[data-testid="token-create-form"]').trigger("submit.prevent");
    await flushPromises();

    expect(wrapper.get('[data-testid="token-output"]').text()).toBe("pkgly_secret_value");
    expect(wrapper.text()).toContain("Expires");
    expect(wrapper.text()).toContain("will not be able to view it again");

    await wrapper.get('[data-testid="copy-token-button"]').trigger("click");
    expect(writeText).toHaveBeenCalledWith("pkgly_secret_value");
  });
});
