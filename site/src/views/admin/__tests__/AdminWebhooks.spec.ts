import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";

vi.mock("@vue/devtools-kit", () => ({}));

const httpGet = vi.fn();
const httpPost = vi.fn();
const httpPut = vi.fn();
const httpDelete = vi.fn();

vi.mock("@/http", () => ({
  default: {
    get: httpGet,
    post: httpPost,
    put: httpPut,
    delete: httpDelete,
  },
}));

const mockAlerts = {
  success: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

const inputStub = defineComponent({
  props: ["modelValue", "id", "disabled", "placeholder", "list"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <slot />
      <input
        :id="id"
        :value="modelValue"
        :disabled="disabled"
        :placeholder="placeholder"
        :list="list"
        @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
});

const switchStub = defineComponent({
  props: ["modelValue", "id", "disabled"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <input
        :id="id"
        type="checkbox"
        :checked="modelValue"
        :disabled="disabled"
        @change="$emit('update:modelValue', $event.target.checked)" />
      <slot />
    </label>
  `,
});

const submitButtonStub = defineComponent({
  props: ["disabled", "loading", "title", "block"],
  emits: ["click"],
  setup(props, { emit, slots }) {
    return () =>
      h(
        "button",
        {
          class: "submit-button",
          type: "submit",
          disabled: props.disabled,
          title: props.title,
          onClick: (event: MouseEvent) => emit("click", event),
        },
        slots.default ? slots.default() : undefined,
      );
  },
});

const stubs = {
  TextInput: inputStub,
  PasswordInput: inputStub,
  SwitchInput: switchStub,
  SubmitButton: submitButtonStub,
  SpinnerElement: defineComponent({ template: "<div data-testid='spinner'></div>" }),
  FloatingErrorBanner: defineComponent({
    props: ["visible", "title", "message"],
    template: "<div data-testid='error-banner' v-if='visible'>{{ title }} {{ message }}</div>",
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: ["disabled", "variant", "color", "prependIcon"],
    emits: ["click"],
    setup(props, { attrs, emit, slots }) {
      return () =>
        h(
          "button",
          {
            class: ["v-btn", attrs.class],
            type: "button",
            disabled: props.disabled,
            onClick: (event: MouseEvent) => emit("click", event),
          },
          slots.default ? slots.default() : undefined,
        );
    },
  }),
};

describe("AdminWebhooks.vue", () => {
  beforeEach(() => {
    httpGet.mockReset();
    httpPost.mockReset();
    httpPut.mockReset();
    httpDelete.mockReset();
    mockAlerts.success.mockReset();

    httpGet.mockImplementation((url: string) => {
      if (url === "/api/system/webhooks") {
        return Promise.resolve({
          data: [
            {
              id: "11111111-1111-1111-1111-111111111111",
              name: "Packages",
              enabled: true,
              target_url: "https://example.com/hooks",
              events: ["package.published"],
              headers: [{ name: "X-Token", configured: true }],
              last_delivery_status: "delivered",
              last_delivery_at: "2026-04-22T10:00:00Z",
              last_http_status: 204,
              last_error: null,
            },
          ],
        });
      }
      throw new Error(`unexpected GET ${url}`);
    });
  });

  it("loads webhook settings without fetching single sign on settings", async () => {
    const module = await import("@/views/admin/AdminWebhooks.vue");
    const wrapper = mount(module.default, {
      global: {
        stubs,
      },
    });

    await flushPromises();

    expect(wrapper.text()).toContain("Package Webhooks");
    expect(wrapper.text()).toContain("Packages");
    expect(httpGet).toHaveBeenCalledWith("/api/system/webhooks");
    expect(httpGet).not.toHaveBeenCalledWith("/api/security/sso");
    expect(httpGet).not.toHaveBeenCalledWith("/api/security/oauth2");
  });

  it("preserves configured header secrets on update", async () => {
    const module = await import("@/views/admin/AdminWebhooks.vue");
    const wrapper = mount(module.default, {
      global: {
        stubs,
      },
    });

    await flushPromises();

    expect((wrapper.get("#webhook-target-url").element as HTMLInputElement).value).toBe(
      "https://example.com/hooks",
    );

    await wrapper.get("#webhook-target-url").setValue("https://example.com/hooks/v2");
    httpPut.mockResolvedValue({ status: 200 });

    await wrapper.get("form.webhookForm").trigger("submit");
    await flushPromises();

    expect(httpPut).toHaveBeenCalledWith(
      "/api/system/webhooks/11111111-1111-1111-1111-111111111111",
      expect.objectContaining({
        name: "Packages",
        target_url: "https://example.com/hooks/v2",
        headers: [{ name: "X-Token", value: null, configured: true }],
      }),
    );
    expect(mockAlerts.success).toHaveBeenCalledWith("Webhook updated");
  });
});
