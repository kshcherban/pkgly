// ABOUTME: Verifies user creation form layout, behavior, and API error feedback.
// ABOUTME: Covers permission defaults, complete payloads, and field-specific failures.
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import http from "@/http";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/http", () => ({
  default: {
    post: vi.fn(),
  },
}));

vi.mock("@/stores/site", () => ({
  siteStore: () => ({
    siteInfo: { version: "test" },
    getInfo: vi.fn(),
    getPasswordRulesOrDefault: () => ({
      min_length: 12,
      require_uppercase: true,
      require_lowercase: true,
      require_number: true,
      require_special: true,
    }),
  }),
}));

const mockAlerts = {
  success: vi.fn(),
  error: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

class LocalStorageMock {
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

const mockLocalStorage = new LocalStorageMock() as unknown as Storage;

Object.defineProperty(globalThis, "localStorage", {
  value: mockLocalStorage,
  configurable: true,
});

Object.defineProperty(window, "localStorage", {
  value: mockLocalStorage,
  configurable: true,
});

let UserCreateView: any;

beforeEach(async () => {
  mockAlerts.success.mockReset();
  mockAlerts.error.mockReset();
  vi.mocked(http.post).mockReset();
  const module = await import("@/views/admin/user/UserCreateView.vue");
  UserCreateView = module.default;
});

afterEach(() => {
  vi.resetModules();
});
const formFieldStubs = {
  TextInput: defineComponent({
    props: ["modelValue", "label", "disabled", "id", "hint"],
    emits: ["update:modelValue"],
    template: `
      <label class="text-input">
        <slot />
        <input
          :id="id"
          :disabled="disabled"
          :value="modelValue"
          @input="$emit('update:modelValue', $event.target.value)" />
      </label>
    `,
  }),
  ValidatableTextBox: defineComponent({
    props: ["modelValue", "disabled", "id", "optional"],
    emits: ["update:modelValue", "validity"],
    setup(_, { emit }) {
      const value = ref("");
      return { emit, value };
    },
    template: `
      <label class="validatable-text-box">
        <slot />
        <input
          :id="id"
          :value="modelValue"
          @input="$emit('update:modelValue', $event.target.value); $emit('validity', true)" />
      </label>
    `,
  }),
  NewPasswordInput: defineComponent({
    props: ["modelValue", "disabled"],
    emits: ["update:modelValue"],
    template: `
      <div data-testid="password-input">
        <label for="password"><slot /></label>
        <input
          id="password"
          type="password"
          :value="modelValue"
          @input="$emit('update:modelValue', $event.target.value)" />
      </div>
    `,
  }),
  SwitchInput: defineComponent({
    props: ["modelValue", "id", "disabled"],
    emits: ["update:modelValue", "change"],
    template: `
      <label :for="id" class="switch-input">
        <slot />
        <input
          type="checkbox"
          :id="id"
          :disabled="disabled"
          :checked="modelValue"
          @change="$emit('update:modelValue', $event.target.checked); $emit('change', $event.target.checked)" />
      </label>
    `,
  }),
};

const vuetifyStubs = {
  "v-container": defineComponent({
    template: "<div data-testid='user-create-container'><slot /></div>",
  }),
  "v-card": defineComponent({
    template: "<div data-testid='user-create-card'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-card-text": defineComponent({
    template: "<div class='v-card-text'><slot /></div>",
  }),
  "v-form": defineComponent({
    emits: ["submit"],
    template:
      "<form data-testid='user-create-form' @submit.prevent=\"$emit('submit', $event)\"><slot /></form>",
  }),
  "v-row": defineComponent({
    template: "<div class='v-row'><slot /></div>",
  }),
  "v-col": defineComponent({
    props: { cols: [Number, String], md: [Number, String] },
    template: "<div class='v-col'><slot /></div>",
  }),
  "v-alert": defineComponent({
    template: "<div :data-testid='$attrs[\"data-testid\"] || \"user-create-error\"'><slot /></div>",
  }),
  "v-btn": defineComponent({
    props: { type: { type: String, default: "button" } },
    emits: ["click"],
    template: "<button class='v-btn' :type='type' @click=\"$emit('click', $event)\"><slot /></button>",
  }),
  "v-spacer": defineComponent({
    template: "<span data-testid='spacer'></span>",
  }),
  "v-divider": defineComponent({
    template: "<hr class='v-divider' />",
  }),
  "v-expand-transition": defineComponent({
    template: "<div><slot /></div>",
  }),
  "v-icon": defineComponent({
    template: "<span><slot /></span>",
  }),
};

describe("UserCreateView.vue", () => {
  it("renders the modern card layout with a visible submit button", async () => {
    const wrapper = mount(UserCreateView, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...formFieldStubs,
        },
      },
    });

    await flushPromises();

    expect(wrapper.find('[data-testid="user-create-card"]').exists()).toBe(true);
    expect(wrapper.find(".v-btn").text()).toContain("Create User");
    expect(wrapper.get('label[for="password"]').text()).toBe("Password");
  });

  it("defaults new users to read-only repository access", async () => {
    const wrapper = mount(UserCreateView, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...formFieldStubs,
        },
      },
    });

    await flushPromises();

    expect((wrapper.get("#admin").element as HTMLInputElement).checked).toBe(false);
    expect((wrapper.get("#userManager").element as HTMLInputElement).checked).toBe(false);
    expect((wrapper.get("#systemManager").element as HTMLInputElement).checked).toBe(false);
    expect((wrapper.get("#defaultRead").element as HTMLInputElement).checked).toBe(true);
    expect((wrapper.get("#defaultWrite").element as HTMLInputElement).checked).toBe(false);
    expect((wrapper.get("#defaultEdit").element as HTMLInputElement).checked).toBe(false);
  });

  it("submits identity, password, and changed permissions together", async () => {
    vi.mocked(http.post).mockResolvedValue({ data: {} });
    const wrapper = mount(UserCreateView, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...formFieldStubs,
        },
      },
    });

    await flushPromises();

    await wrapper.get("#username").setValue("test1");
    await wrapper.get("#email").setValue("test@example.com");
    await wrapper.get('input[type="password"]').setValue("P@ssw0rd!Test123");
    await wrapper.get("#userManager").setValue(true);
    await wrapper.get("#defaultRead").setValue(false);
    await wrapper.get("#defaultWrite").setValue(true);
    await wrapper.get('[data-testid="user-create-form"]').trigger("submit");
    await flushPromises();

    expect(http.post).toHaveBeenCalledWith("/api/user-management/create", {
      name: "test1",
      email: "test@example.com",
      username: "test1",
      password: "P@ssw0rd!Test123",
      permissions: {
        admin: false,
        user_manager: true,
        system_manager: false,
        default_repository_actions: ["Write"],
      },
    });
  });

  it("shows the conflicting username returned by the create API", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(http.post).mockRejectedValue({
      isAxiosError: true,
      response: {
        status: 409,
        data: {
          message: "username already exists",
          details: "username",
        },
      },
      toJSON: () => ({ status: 409 }),
    });
    const wrapper = mount(UserCreateView, {
      global: {
        stubs: {
          ...vuetifyStubs,
          ...formFieldStubs,
        },
      },
    });

    await flushPromises();

    await wrapper.get("#username").setValue("test1");
    await wrapper.get("#email").setValue("test@example.com");
    await wrapper.get('input[type="password"]').setValue("P@ssw0rd!Test123");
    await wrapper.get('[data-testid="user-create-form"]').trigger("submit");
    await flushPromises();

    expect(http.post).toHaveBeenCalledWith("/api/user-management/create", {
      name: "test1",
      email: "test@example.com",
      username: "test1",
      password: "P@ssw0rd!Test123",
      permissions: {
        admin: false,
        user_manager: false,
        system_manager: false,
        default_repository_actions: ["Read"],
      },
    });
    expect(wrapper.get('[data-testid="user-create-error"]').text()).toContain(
      "Username already exists",
    );
    consoleError.mockRestore();
  });
});
