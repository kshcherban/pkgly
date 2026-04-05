import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick } from "vue";
import UserPermissions from "@/components/admin/user/UserPermissions.vue";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/http", () => ({
  default: {
    put: vi.fn().mockResolvedValue(undefined),
  },
}));

const SubmitButtonStub = defineComponent({
  props: {
    block: {
      type: Boolean,
      default: true,
    },
    disabled: Boolean,
  },
  emits: ["click"],
  template: `<button class="submit-button-stub" :data-block="block" :disabled="disabled" @click="$emit('click')"><slot /></button>`,
});

const SwitchInputStub = defineComponent({
  props: {
    modelValue: Boolean,
    id: String,
  },
  emits: ["update:modelValue"],
  template: `
    <label class="switch-input-stub">
      <input
        type="checkbox"
        :checked="modelValue"
        @change="$emit('update:modelValue', $event.target.checked)" />
      <slot />
    </label>
  `,
});

const defaultUser = {
  id: 1,
  name: "User",
  username: "user",
  email: "user@example.com",
  admin: false,
  user_manager: false,
  system_manager: false,
  default_repository_actions: [],
};

describe("UserPermissions.vue", () => {
  it("places the save button in the footer with non-block sizing", async () => {
    const wrapper = mount(UserPermissions, {
      props: {
        user: defaultUser,
      },
      global: {
        stubs: {
          SubmitButton: SubmitButtonStub,
          SwitchInput: SwitchInputStub,
        },
      },
    });

    await flushPromises();

    const footer = wrapper.find(".user-permissions__actions");
    expect(footer.exists()).toBe(true);

    const button = footer.findComponent(SubmitButtonStub);
    expect(button.exists()).toBe(true);
    expect(button.props("block")).toBe(false);
  });

  it("enables save when a switch changes", async () => {
    const wrapper = mount(UserPermissions, {
      props: {
        user: defaultUser,
      },
      global: {
        stubs: {
          SubmitButton: SubmitButtonStub,
          SwitchInput: SwitchInputStub,
        },
      },
    });

    const switchInput = wrapper.find("input[type='checkbox']");
    expect(switchInput.exists()).toBe(true);

    await switchInput.setValue(true);
    await nextTick();

    const button = wrapper.findComponent(SubmitButtonStub);
    expect(button.attributes("disabled")).toBeUndefined();
  });
});
