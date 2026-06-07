// ABOUTME: Tests the admin user list table component's search, header columns, and admin status rendering.
// ABOUTME: Verifies that the Admin column appears and maps boolean admin flags to readable labels.
import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick } from "vue";
vi.mock("@vue/devtools-kit", () => ({}));

const localStorageMock = {
  getItem: vi.fn().mockReturnValue(null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
  key: vi.fn(),
  length: 0,
};

Object.defineProperty(globalThis, "localStorage", {
  value: localStorageMock,
  configurable: true,
});

Object.defineProperty(window, "localStorage", {
  value: localStorageMock,
  configurable: true,
});

const vuetifyStubs = {
  "v-card": defineComponent({
    template: "<div class='v-card'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-spacer": defineComponent({
    template: "<span class='v-spacer' />",
  }),
  "v-text-field": defineComponent({
    props: {
      modelValue: {
        type: String,
        default: "",
      },
      clearable: {
        type: Boolean,
        default: false,
      },
    },
    emits: ["update:modelValue", "click:clear"],
    template: `
      <label class="v-text-field">
        <input
          :value="modelValue"
          @input="$emit('update:modelValue', $event.target.value)" />
        <button
          type="button"
          class="v-text-field__clear"
          @click="$emit('click:clear')">
          clear
        </button>
        <slot />
      </label>
    `,
  }),
  "v-data-table": defineComponent({
    props: {
      headers: Array,
      items: Array,
    },
    emits: ["click:row"],
    template: "<table class='v-data-table'><slot /></table>",
  }),
  "v-chip": defineComponent({
    template: "<span class='v-chip'><slot /></span>",
  }),
};

describe("UserList.vue", () => {
  let UserList: any;

  beforeEach(async () => {
    const module = await import("@/components/admin/user/UserList.vue");
    UserList = module.default;
  });

  it("sets search input clearable and handles clear action", async () => {
    const wrapper = mount(UserList, {
      props: {
        users: [
          {
            id: 1,
            name: "Alice",
            username: "alice",
            active: true,
            admin: false,
            email: "alice@example.com",
          },
        ],
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    const field = wrapper.getComponent(vuetifyStubs["v-text-field"]);
    expect(field.props("clearable")).toBe(true);

    field.vm.$emit("update:modelValue", "Ali");
    await nextTick();
    expect(wrapper.vm.searchValue).toBe("Ali");

    field.vm.$emit("click:clear");
    await nextTick();
    expect(wrapper.vm.searchValue).toBe("");
  });

  it("includes Admin column in table headers", async () => {
    const wrapper = mount(UserList, {
      props: {
        users: [
          { id: 1, name: "Alice", username: "alice", active: true, admin: false, email: "alice@example.com" },
        ],
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    const headers = wrapper.getComponent(vuetifyStubs["v-data-table"]).props("headers") as Array<{ title: string }>;
    expect(headers.map((h) => h.title)).toEqual(["ID #", "Name", "Username", "Admin", "Status"]);
    expect(headers.map((h) => h.title)).toContain("Admin");
  });

  it("maps admin boolean to readable labels in table items", async () => {
    const wrapper = mount(UserList, {
      props: {
        users: [
          { id: 1, name: "Admin User", username: "admin", active: true, admin: true, email: "admin@example.com" },
          { id: 2, name: "Normal", username: "user1", active: true, admin: false, email: "user@example.com" },
        ],
      },
      global: {
        stubs: vuetifyStubs,
      },
    });

    const items = wrapper.vm.tableItems as Array<{ admin: boolean; admin_label: string }>;
    expect(items[0].admin).toBe(true);
    expect(items[0].admin_label).toBe("Admin");
    expect(items[1].admin).toBe(false);
    expect(items[1].admin_label).toBe("User");
  });
});
