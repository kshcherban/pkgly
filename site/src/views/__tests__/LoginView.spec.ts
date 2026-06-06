// ABOUTME: Verifies local login errors, accessible controls, and session handoff.
// ABOUTME: Keeps authentication tests isolated from global alerts and routing.
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockAlerts = {
  error: vi.fn(),
  success: vi.fn(),
};

const mockSession = {
  login: vi.fn(),
};

const mockRouter = {
  push: vi.fn(),
};

const httpPost = vi.fn();

vi.mock("@/http", () => ({
  default: {
    post: httpPost,
  },
}));

vi.mock("@/router", () => ({
  default: mockRouter,
}));

vi.mock("@/stores/session", () => ({
  sessionStore: () => mockSession,
}));

vi.mock("@/stores/site", () => ({
  siteStore: () => ({
    siteInfo: {},
    getInfo: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

vi.mock("vue-router", async () => {
  const actual = await vi.importActual("vue-router");
  return {
    ...actual,
    useRoute: () => ({
      query: {},
    }),
  };
});

const VFormStub = {
  emits: ["submit"],
  template: `<form data-testid="login-form" @submit.prevent="$emit('submit', $event)"><slot /></form>`,
};

const VTextFieldStub = {
  inheritAttrs: false,
  props: ["id", "modelValue", "type"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <input
        :id="id"
        :type="type || 'text'"
        :value="modelValue"
        @input="$emit('update:modelValue', $event.target.value)" />
      <slot name="append-inner" />
    </label>
  `,
};

async function mountLogin() {
  const LoginView = (await import("@/views/LoginView.vue")).default;
  return mount(LoginView, {
    global: {
      stubs: {
        "router-link": { template: "<a><slot /></a>" },
        "v-container": { template: "<div><slot /></div>" },
        "v-row": { template: "<div><slot /></div>" },
        "v-col": { template: "<div><slot /></div>" },
        "v-card": { template: "<div><slot /></div>" },
        "v-card-title": { template: "<div><slot /></div>" },
        "v-card-text": { template: "<div><slot /></div>" },
        "v-avatar": { template: "<div />" },
        "v-btn": {
          template: "<button :type=\"$attrs.type\" :aria-label=\"$attrs['aria-label']\"><slot /></button>",
        },
        "v-divider": { template: "<div><slot /></div>" },
        "v-form": VFormStub,
        "v-alert": { template: "<div data-testid='login-inline-alert'><slot /></div>" },
        "v-text-field": VTextFieldStub,
        "v-icon": { template: "<i><slot /></i>" },
        "v-tooltip": { template: "<span><slot name='activator' :props='{}' /></span>" },
      },
    },
  });
}

describe("LoginView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the inline login error without triggering a global alert on 401", async () => {
    httpPost.mockRejectedValueOnce({
      response: {
        status: 401,
      },
    });

    const wrapper = await mountLogin();

    await wrapper.get("#login-username").setValue("admin");
    await wrapper.get("#login-password").setValue("wrong-password");
    await wrapper.get('[data-testid="login-form"]').trigger("submit");
    await flushPromises();

    expect(wrapper.get('[data-testid="login-inline-alert"]').text()).toContain(
      "Invalid username or password",
    );
    expect(mockAlerts.error).not.toHaveBeenCalled();
    expect(mockSession.login).not.toHaveBeenCalled();
    expect(mockRouter.push).not.toHaveBeenCalled();
  });

  it("gives password visibility and submit controls explicit accessible names", async () => {
    const wrapper = await mountLogin();

    expect(wrapper.findAll('label[for="login-username"]')).toHaveLength(1);
    expect(wrapper.findAll('label[for="login-password"]')).toHaveLength(1);
    expect(wrapper.get('button[aria-label="Show password"]').exists()).toBe(true);
    expect(wrapper.get('button[aria-label="Log in"]').attributes("type")).toBe("submit");

    await wrapper.get('button[aria-label="Show password"]').trigger("click");
    expect(wrapper.get('button[aria-label="Hide password"]').exists()).toBe(true);
    expect(wrapper.get("#login-password").attributes("type")).toBe("text");
  });
});
