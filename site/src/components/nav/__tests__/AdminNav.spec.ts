import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import AdminNav from "@/components/nav/AdminNav.vue";

let currentRoute = {
  name: "RepositoriesList",
  meta: {
    tag: "admin-repositories",
  },
};

vi.mock("vue-router", () => ({
  useRouter: () => ({
    currentRoute: {
      value: currentRoute,
    },
  }),
}));

const stubs = {
  RouterLink: defineComponent({
    props: {
      to: {
        type: String,
        required: true,
      },
    },
    template: "<a :href='to'><slot /></a>",
  }),
  FontAwesomeIcon: defineComponent({
    template: "<span />",
  }),
};

describe("AdminNav.vue", () => {
  beforeEach(() => {
    currentRoute = {
      name: "RepositoriesList",
      meta: {
        tag: "admin-repositories",
      },
    };
  });

  it("renders section navigation without duplicate create actions", () => {
    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.text()).toContain("Users");
    expect(wrapper.text()).toContain("Storages");
    expect(wrapper.text()).toContain("Repositories");
    expect(wrapper.text()).toContain("System");
    expect(wrapper.text()).toContain("Single Sign On");
    expect(wrapper.text()).toContain("Webhooks");
    expect(wrapper.text()).not.toContain("Create User");
    expect(wrapper.text()).not.toContain("Create Storage");
    expect(wrapper.text()).not.toContain("Create Repository");
  });

  it("links to system settings subpages", () => {
    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.get('a[href="/admin/system/sso"]').text()).toContain("Single Sign On");
    expect(wrapper.get('a[href="/admin/system/webhooks"]').text()).toContain("Webhooks");
  });

  it("keeps the parent section active on create routes", () => {
    currentRoute = {
      name: "RepositoryCreate",
      meta: {
        tag: "admin-repositories",
      },
    };

    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    const repositoriesLink = wrapper.get('a[href="/admin/repositories"]');
    expect(repositoriesLink.attributes("data-active")).toBe("true");
  });

  it("marks the active system subpage", () => {
    currentRoute = {
      name: "SystemWebhooks",
      meta: {},
    };

    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.get('a[href="/admin/system/webhooks"]').attributes("data-active")).toBe("true");
    expect(wrapper.get('a[href="/admin/system/sso"]').attributes("data-active")).toBe("false");
  });
});
