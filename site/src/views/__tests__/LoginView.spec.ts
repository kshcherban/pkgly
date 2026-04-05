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
  props: ["modelValue", "label", "type"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <span>{{ label }}</span>
      <input
        :aria-label="label"
        :type="type || 'text'"
        :value="modelValue"
        @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
};

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

    const LoginView = (await import("@/views/LoginView.vue")).default;
    const wrapper = mount(LoginView, {
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
          "v-btn": { template: "<button :type=\"$attrs.type\"><slot /></button>" },
          "v-divider": { template: "<div><slot /></div>" },
          "v-form": VFormStub,
          "v-alert": { template: "<div data-testid='login-inline-alert'><slot /></div>" },
          "v-text-field": VTextFieldStub,
          "v-icon": { template: "<i />" },
        },
      },
    });

    await wrapper.get('input[aria-label="Username or Email"]').setValue("admin");
    await wrapper.get('input[aria-label="Password"]').setValue("wrong-password");
    await wrapper.get('[data-testid="login-form"]').trigger("submit");
    await flushPromises();

    expect(wrapper.get('[data-testid="login-inline-alert"]').text()).toContain(
      "Invalid username or password",
    );
    expect(mockAlerts.error).not.toHaveBeenCalled();
    expect(mockSession.login).not.toHaveBeenCalled();
    expect(mockRouter.push).not.toHaveBeenCalled();
  });
});
