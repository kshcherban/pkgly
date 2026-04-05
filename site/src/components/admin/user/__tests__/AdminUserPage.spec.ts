import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick } from "vue";
import { flushPromises } from "@vue/test-utils";
import SubmitButton from "@/components/form/SubmitButton.vue";
vi.mock("@vue/devtools-kit", () => ({}));

const mockAlerts = {
  success: vi.fn(),
  error: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
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

vi.mock("@/stores/session", () => ({
  sessionStore: () => ({
    user: { id: 999, admin: true },
  }),
}));

vi.mock("@/http", () => ({
  default: {
    put: vi.fn().mockResolvedValue(undefined),
    delete: vi.fn().mockResolvedValue(undefined),
  },
}));

beforeEach(() => {
  mockAlerts.success.mockReset();
  mockAlerts.error.mockReset();
});

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

const VBtnStub = defineComponent({
  props: {
    type: {
      type: String,
      default: "button",
    },
    color: String,
    variant: String,
    block: Boolean,
    loading: Boolean,
    disabled: Boolean,
    prependIcon: String,
  },
  emits: ["click"],
  template: `
    <button
      class="v-btn"
      :type="type"
      :disabled="disabled"
      @click="$emit('click', $event)">
      <slot />
    </button>
  `,
});

const VTabsStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    density: {
      type: String,
      default: "default",
    },
  },
  emits: ["update:modelValue"],
  template: `
    <div class="v-tabs" data-testid="admin-user-tabs">
      <slot />
    </div>
  `,
});

const VTabStub = defineComponent({
  props: {
    value: {
      type: String,
      required: true,
    },
  },
  template: `
    <button class="v-tab" data-testid="admin-user-tab">
      <slot />
    </button>
  `,
});

const VWindowStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
  },
  emits: ["update:modelValue"],
  template: `<div class="v-window"><slot /></div>`,
});

const VWindowItemStub = defineComponent({
  props: {
    value: {
      type: String,
      required: true,
    },
  },
  template: `<div class="v-window-item"><slot /></div>`,
});

const VCardStub = defineComponent({
  template: `<div class="v-card"><slot /></div>`,
});

const VContainerStub = defineComponent({
  template: `<div class="v-container"><slot /></div>`,
});

const VDividerStub = defineComponent({
  template: `<hr class="v-divider" />`,
});

const FloatingErrorBannerStub = defineComponent({
  props: {
    visible: Boolean,
    title: String,
    message: String,
  },
  emits: ["close"],
  template: `<div v-if="visible" class="floating-error-banner"><slot /></div>`,
});

const TextInputStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    id: String,
  },
  emits: ["update:modelValue"],
  template: `
    <label class="text-input">
      <slot />
      <input :id="id" :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
});

const ValidatableTextBoxStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    id: String,
  },
  emits: ["update:modelValue"],
  template: `
    <label class="validatable-text-box">
      <slot />
      <input :id="id" :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
});

const NewPasswordInputStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    id: String,
  },
  emits: ["update:modelValue"],
  template: `
    <label class="password-input">
      <slot />
      <input
        type="password"
        :id="id"
        :value="modelValue"
        @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
});

const KeyAndValueStub = defineComponent({
  props: {
    label: String,
    value: String,
  },
  template: `<dl class="key-value"><dt>{{ label }}</dt><dd>{{ value }}</dd></dl>`,
});

const UserPermissionsStub = defineComponent({
  props: { user: Object },
  template: `<div class="user-permissions-stub">User Permissions</div>`,
});

const RepositoryPermissionsStub = defineComponent({
  props: { user: Object },
  template: `<div class="repository-permissions-stub">Repo Permissions</div>`,
});

let AdminUserPage: any;

const defaultUser = {
  id: 1,
  name: "Test User",
  username: "test-user",
  email: "test@example.com",
  created_at: "2025-11-10T21:19:40Z",
  active: true,
};

function factory() {
  if (!AdminUserPage) {
    throw new Error("AdminUserPage component not loaded");
  }
  return mount(AdminUserPage, {
    props: {
      user: defaultUser,
    },
    global: {
      stubs: {
        "v-btn": VBtnStub,
        "v-tabs": VTabsStub,
        "v-tab": VTabStub,
        "v-window": VWindowStub,
        "v-window-item": VWindowItemStub,
        "v-card": VCardStub,
        "v-container": VContainerStub,
        "v-divider": VDividerStub,
        FloatingErrorBanner: FloatingErrorBannerStub,
        TextInput: TextInputStub,
        ValidatableTextBox: ValidatableTextBoxStub,
        NewPasswordInput: NewPasswordInputStub,
        KeyAndValue: KeyAndValueStub,
        UserPermissions: UserPermissionsStub,
        RepositoryPermissions: RepositoryPermissionsStub,
      },
    },
  });
}

describe("AdminUserPage.vue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  beforeEach(async () => {
    const module = await import("@/components/admin/user/AdminUserPage.vue");
    AdminUserPage = module.default;
  });

  it("uses Vuetify tab components for navigation", () => {
    const wrapper = factory();
    const tabs = wrapper.find('[data-testid="admin-user-tabs"]');
    expect(tabs.exists()).toBe(true);
    expect(wrapper.findAll('[data-testid="admin-user-tab"]').length).toBe(4);
  });

  it("renders save buttons with consistent non-block sizing", () => {
    const wrapper = factory();
    const buttons = wrapper.findAllComponents(SubmitButton);
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.props("block")).toBe(false);
    }
  });

  it("expands the password form area for easier entry", async () => {
    const wrapper = factory();
    (wrapper.vm as any).currentTab = "password";
    await nextTick();
    const passwordForm = wrapper.find('[data-testid="admin-user-password-form"]');
    expect(passwordForm.exists()).toBe(true);
    expect(passwordForm.classes()).toContain("admin-user-page__password-form");
  });

  it("submits user details without leaving the admin page", async () => {
    const wrapper = factory();
    const http = await import("@/http");

    await wrapper.find("#email").setValue("updated@example.com");
    await wrapper.find("#username").setValue("updated-user");
    await wrapper.find(".admin-user-page__form").trigger("submit");

    await flushPromises();

    expect(http.default.put).toHaveBeenCalledWith(
      "/api/user-management/update/1",
      expect.objectContaining({
        email: "updated@example.com",
        username: "updated-user",
      }),
    );
    expect(mockAlerts.success).toHaveBeenCalledWith(
      "User updated",
      "User profile details have been saved.",
    );
  });

  it("renders the floating error without a global toast when saving fails", async () => {
    const wrapper = factory();
    const http = await import("@/http");

    (http.default.put as unknown as ReturnType<typeof vi.fn>).mockRejectedValueOnce({
      response: {
        status: 409,
        data: {
          message: "Username already exists.",
        },
      },
      toJSON: () => ({}),
    });

    await wrapper.find(".admin-user-page__form").trigger("submit");
    await flushPromises();

    expect(wrapper.find(".floating-error-banner").exists()).toBe(true);
    expect(mockAlerts.error).not.toHaveBeenCalled();
  });
});
