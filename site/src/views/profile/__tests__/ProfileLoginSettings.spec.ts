import { mount } from "@vue/test-utils";
import { describe, expect, it, beforeEach, vi } from "vitest";
import { defineComponent, h, nextTick, ref, watch } from "vue";

const getInfoMock = vi.fn();
vi.mock("@/stores/site", () => ({
  siteStore: vi.fn(() => ({
    siteInfo: {
      password_rules: {
        minLength: 8,
        requireUppercase: true,
        requireLowercase: true,
        requireNumber: true,
        requireSymbol: false,
      },
    },
    getInfo: getInfoMock,
  })),
}));

const sessionUser = {
  email: "user@example.com",
  username: "testuser",
};

const sessionState = {
  user: sessionUser,
};

vi.mock("@/stores/session", () => ({
  sessionStore: vi.fn(() => sessionState),
}));

const postMock = vi.fn();
vi.mock("@/http", () => ({
  default: {
    post: postMock,
  },
}));

const localStorageMock = {
  getItem: vi.fn(),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
};
vi.stubGlobal("localStorage", localStorageMock);

const SubmitButtonStub = defineComponent({
  name: "SubmitButtonStub",
  props: {
    disabled: Boolean,
    block: Boolean,
  },
  emits: ["click"],
  setup(props, { slots, emit }) {
    return () =>
      h(
        "button",
        {
          class: "submit-button-stub",
          disabled: props.disabled,
          onClick: (event: MouseEvent) => emit("click", event),
        },
        slots.default ? slots.default() : undefined,
      );
  },
});

const PasswordInputStub = defineComponent({
  name: "PasswordInputStub",
  props: {
    modelValue: String,
    id: {
      type: String,
      required: true,
    },
  },
  emits: ["update:modelValue"],
  setup(props, { emit }) {
    return () =>
      h("input", {
        id: props.id,
        type: "password",
        value: props.modelValue,
        onInput: (event: Event) => emit("update:modelValue", (event.target as HTMLInputElement).value),
      });
  },
});

const TextInputStub = defineComponent({
  name: "TextInputStub",
  props: {
    modelValue: String,
    id: {
      type: String,
      required: true,
    },
    type: {
      type: String,
      default: "text",
    },
  },
  emits: ["update:modelValue"],
  setup(props, { emit }) {
    return () =>
      h("input", {
        id: props.id,
        type: props.type,
        value: props.modelValue,
        onInput: (event: Event) => emit("update:modelValue", (event.target as HTMLInputElement).value),
      });
  },
});

const NewPasswordInputStub = defineComponent({
  name: "NewPasswordInputStub",
  props: {
    modelValue: String,
    id: {
      type: String,
      required: true,
    },
    passwordRules: Object,
  },
  emits: ["update:modelValue"],
  setup(props, { emit }) {
    const state = ref({ value: props.modelValue ?? "", confirm: props.modelValue ?? "" });
    watch(
      () => props.modelValue,
      (newValue) => {
        state.value = {
          value: newValue ?? "",
          confirm: newValue ?? "",
        };
      },
    );
    return () =>
      h("div", { class: "new-password-stub" }, [
        h("input", {
          id: props.id,
          type: "password",
          value: state.value.value,
          onInput: (event: Event) => {
            state.value.value = (event.target as HTMLInputElement).value;
            emit(
              "update:modelValue",
              state.value.value && state.value.value === state.value.confirm ? state.value.value : undefined,
            );
          },
        }),
        h("input", {
          id: `${props.id}-confirm`,
          type: "password",
          value: state.value.confirm,
          onInput: (event: Event) => {
            state.value.confirm = (event.target as HTMLInputElement).value;
            emit(
              "update:modelValue",
              state.value.value && state.value.value === state.value.confirm ? state.value.value : undefined,
            );
          },
        }),
      ]);
  },
});

describe("ProfileLoginSettings.vue", () => {
  beforeEach(() => {
    sessionState.user = {
      email: "user@example.com",
      username: "testuser",
    };
    postMock.mockReset();
    getInfoMock.mockReset();
    localStorageMock.getItem.mockReset();
    localStorageMock.setItem.mockReset();
    localStorageMock.removeItem.mockReset();
    localStorageMock.clear.mockReset();
  });

  async function factory() {
    const module = await import("@/views/profile/ProfileLoginSettings.vue");
    const ProfileLoginSettings = module.default;

    return mount(ProfileLoginSettings, {
      global: {
        stubs: {
          SubmitButton: SubmitButtonStub,
          TextInput: TextInputStub,
          PasswordInput: PasswordInputStub,
          NewPasswordInput: NewPasswordInputStub,
          "v-progress-circular": { template: "<div class=\"spinner\"></div>" },
          "v-alert": defineComponent({
            props: {
              type: String,
            },
            emits: ["click:close"],
            setup(props, { slots }) {
              return () =>
                h(
                  "div",
                  {
                    class: ["v-alert", props.type].filter(Boolean).join(" "),
                  },
                  slots.default ? slots.default() : undefined,
                );
            },
          }),
          "v-container": { template: "<div class=\"v-container\"><slot /></div>" },
          "v-row": { template: "<div class=\"v-row\"><slot /></div>" },
          "v-col": { template: "<div class=\"v-col\"><slot /></div>" },
          "v-card": { template: "<div class=\"v-card\"><slot /></div>" },
          "v-card-title": { template: "<div class=\"v-card-title\"><slot /></div>" },
          "v-card-text": { template: "<div class=\"v-card-text\"><slot /></div>" },
          "v-divider": { template: "<hr class=\"v-divider\" />" },
        },
      },
    });
  }

  it("disables submit until fields are populated and matching", async () => {
    const wrapper = await factory();
    const submit = wrapper.find('[data-testid="password-form"] .submit-button-stub');
    expect(submit.attributes("disabled")).toBeDefined();

    await wrapper.find("#currentPassword").setValue("oldPass123");
    await wrapper.find("#newPassword").setValue("NewPass123");
    await nextTick();

    // Without confirmation the button stays disabled
    expect(submit.attributes("disabled")).toBeDefined();

    await wrapper.find("#newPassword-confirm").setValue("NewPass123");
    (wrapper.vm as any).newPassword = "NewPass123";
    await nextTick();

    expect(submit.attributes("disabled")).toBeUndefined();
  });

  it("submits password change and resets fields", async () => {
    postMock.mockResolvedValue({});
    const wrapper = await factory();

    await wrapper.find("#currentPassword").setValue("oldPass123");
    await wrapper.find("#newPassword").setValue("NewPass123");
    await wrapper.find("#newPassword-confirm").setValue("NewPass123");
    (wrapper.vm as any).newPassword = "NewPass123";
    await nextTick();

    await wrapper.find('[data-testid="password-form"]').trigger("submit.prevent");

    expect(postMock).toHaveBeenCalledWith("/api/user/change-password", {
      old_password: "oldPass123",
      new_password: "NewPass123",
    });
    const alert = wrapper.find(".v-alert.success");
    expect(alert.exists()).toBe(true);
    expect(alert.text()).toContain("Password updated");
    expect((wrapper.find("#currentPassword").element as HTMLInputElement).value).toBe("");
    expect((wrapper.find("#newPassword").element as HTMLInputElement).value).toBe("");
    expect((wrapper.find("#newPassword-confirm").element as HTMLInputElement).value).toBe("");
    const submit = wrapper.find('[data-testid="password-form"] .submit-button-stub');
    expect(submit.attributes("disabled")).toBeDefined();
  });

  it("submits email change and updates the session user", async () => {
    postMock.mockResolvedValue({
      data: {
        email: "updated@example.com",
        username: "testuser",
      },
    });
    const wrapper = await factory();

    await wrapper.find("#profileEmail").setValue("updated@example.com");
    await nextTick();

    const submit = wrapper.find('[data-testid="email-form"] .submit-button-stub');
    expect(submit.attributes("disabled")).toBeUndefined();

    await wrapper.find('[data-testid="email-form"]').trigger("submit.prevent");

    expect(postMock).toHaveBeenCalledWith("/api/user/change-email", {
      email: "updated@example.com",
    });
    expect(sessionState.user.email).toBe("updated@example.com");
    const alert = wrapper.find(".v-alert.success");
    expect(alert.exists()).toBe(true);
    expect(alert.text()).toContain("Email updated");
  });

  it("shows a conflict message when the new email is already in use", async () => {
    postMock.mockRejectedValue({
      isAxiosError: true,
      response: {
        status: 409,
      },
    });
    const wrapper = await factory();

    await wrapper.find("#profileEmail").setValue("taken@example.com");
    await nextTick();
    await wrapper.find('[data-testid="email-form"]').trigger("submit.prevent");

    expect(postMock).toHaveBeenCalledWith("/api/user/change-email", {
      email: "taken@example.com",
    });
    const alert = wrapper.find(".v-alert.error");
    expect(alert.exists()).toBe(true);
    expect(alert.text()).toContain("Email update failed");
    expect(alert.text()).toContain("already in use");
  });

  it("shows an error when confirmation does not match", async () => {
    const wrapper = await factory();

    await wrapper.find("#currentPassword").setValue("oldPass123");
    await wrapper.find("#newPassword").setValue("NewPass123");
    await wrapper.find("#newPassword-confirm").setValue("Different123");
    (wrapper.vm as any).newPassword = undefined;
    await nextTick();

    await wrapper.find('[data-testid="password-form"]').trigger("submit.prevent");

    expect(postMock).not.toHaveBeenCalled();
    const submit = wrapper.find('[data-testid="password-form"] .submit-button-stub');
    expect(submit.attributes("disabled")).toBeDefined();
  });

  it("notifies on API failure", async () => {
    postMock.mockRejectedValue(new Error("boom"));
    const wrapper = await factory();

    await wrapper.find("#currentPassword").setValue("oldPass123");
    await wrapper.find("#newPassword").setValue("NewPass123");
    await wrapper.find("#newPassword-confirm").setValue("NewPass123");
    (wrapper.vm as any).newPassword = "NewPass123";
    await nextTick();

    await wrapper.find('[data-testid="password-form"]').trigger("submit.prevent");

    expect(postMock).toHaveBeenCalled();
    const alert = wrapper.find(".v-alert.error");
    expect(alert.exists()).toBe(true);
    expect(alert.text()).toContain("Password update failed");
  });
});
