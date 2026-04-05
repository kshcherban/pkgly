import { mount, flushPromises } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import UserListView from "@/views/admin/user/UserListView.vue";
import type { UserResponseType } from "@/types/base";
import http from "@/http";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
  },
}));

vi.mock("@/components/admin/user/UserList.vue", () => ({
  default: defineComponent({
    props: {
      users: {
        type: Array,
        default: () => [],
      },
    },
    template: `<div data-testid="user-list" :data-count="users.length"></div>`,
  }),
}));

const users: UserResponseType[] = [
  {
    id: "user-1",
    name: "Jane Doe",
    username: "jdoe",
    email: "jane@example.com",
    active: true,
    admin: false,
    mfa_enabled: false,
    created_at: "2025-10-01T00:00:00Z",
    last_logged_in: null,
    tags: [],
  },
];

const vuetifyStubs = {
  "v-container": defineComponent({
    template: "<div data-testid='container'><slot /></div>",
  }),
  "v-card": defineComponent({
    template: "<div class='v-card'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-card-text": defineComponent({
    template: "<div class='v-card-text'><slot /></div>",
  }),
  "v-btn": defineComponent({
    props: { to: [String, Object] },
    emits: ["click"],
    template: "<button class='v-btn'><slot /></button>",
  }),
  "v-alert": defineComponent({
    template: "<div data-testid='user-error'><slot /></div>",
  }),
  "v-progress-circular": defineComponent({
    template: "<div data-testid='users-loading'></div>",
  }),
  "v-spacer": defineComponent({
    template: "<span data-testid='spacer'></span>",
  }),
  "v-icon": defineComponent({
    props: { color: String, size: [String, Number] },
    template: "<i class='v-icon'><slot /></i>",
  }),
};

describe("UserListView.vue", () => {
  it("shows a loading indicator while fetching users", async () => {
    const deferred = createDeferred<{ data: UserResponseType[] }>();
    (http.get as vi.Mock).mockReturnValue(deferred.promise);

    const wrapper = mount(UserListView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    expect(wrapper.find('[data-testid="users-loading"]').exists()).toBe(true);

    deferred.resolve({ data: users });
    await flushPromises();

    expect(wrapper.find('[data-testid="users-loading"]').exists()).toBe(false);
  });

  it("renders the user list once data loads", async () => {
    (http.get as vi.Mock).mockResolvedValue({ data: users });

    const wrapper = mount(UserListView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const list = wrapper.find('[data-testid="user-list"]');
    expect(list.exists()).toBe(true);
    expect(list.attributes("data-count")).toBe("1");
    expect(wrapper.find('[data-testid="create-user-button"]').exists()).toBe(true);
  });
});

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
