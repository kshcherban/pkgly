import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/http", () => ({
  default: {
    post: vi.fn().mockResolvedValue({ status: 204 }),
  },
}));

vi.mock("@/router", () => ({
  default: {
    replace: vi.fn(),
  },
}));

const fieldStub = {
  template: `<label class="form-field" :id="$attrs.id"><slot /></label>`,
  props: ["modelValue"],
  emits: ["update:modelValue"],
};

const localStorageStub = vi.hoisted(() => {
  const stub = {
    getItem: vi.fn().mockReturnValue(null),
    setItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
    key: vi.fn(),
    length: 0,
  };
  (globalThis as any).localStorage = stub;
  const existingWindow = (globalThis as any).window ?? {};
  (globalThis as any).window = { ...existingWindow, localStorage: stub };
  return stub;
});

describe("InstallView.vue", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("renders a full-width install form layout", async () => {
    const InstallView = (await import("@/views/admin/InstallView.vue")).default;
    const wrapper = mount(InstallView, {
      global: {
        stubs: {
          TextInput: fieldStub,
          PasswordInput: fieldStub,
          SubmitButton: {
            template: `<button><slot /></button>`,
            props: ["disabled"],
          },
        },
      },
    });

    await flushPromises();

    const form = wrapper.get('[data-testid="install-form"]');
    expect(form.classes()).toContain("install-form");

    const fields = form.findAll('[data-testid="install-field"]');
    expect(fields).not.toHaveLength(0);
    for (const field of fields) {
      expect(field.classes()).toContain("install-form__field");
    }
  });

  it("asks only for username and password during first admin setup", async () => {
    const InstallView = (await import("@/views/admin/InstallView.vue")).default;
    const wrapper = mount(InstallView, {
      global: {
        stubs: {
          TextInput: fieldStub,
          PasswordInput: fieldStub,
          SubmitButton: {
            template: `<button><slot /></button>`,
            props: ["disabled"],
          },
        },
      },
    });

    await flushPromises();

    expect(wrapper.find("#username").exists()).toBe(true);
    expect(wrapper.find("#password").exists()).toBe(true);
    expect(wrapper.find("#confirmPassword").exists()).toBe(false);
    expect(wrapper.find("#name").exists()).toBe(false);
    expect(wrapper.find("#email").exists()).toBe(false);
  });
});
