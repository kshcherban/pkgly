// ABOUTME: Verifies admin side navigation links, active states, and metadata footer.
// ABOUTME: Keeps admin navigation behavior stable across route and instance changes.
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import AdminNav from "@/components/nav/AdminNav.vue";
import { siteStore } from "@/stores/site";

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
    setActivePinia(createPinia());
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

  it("renders the package version and commit id in the footer", () => {
    siteStore().siteInfo = {
      mode: "debug",
      name: "Pkgly",
      description: "Repository server",
      is_installed: true,
      version: "1.2.3",
      commit_id: "abc1234",
    };

    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.get(".adminNav__version").text()).toBe("Pkgly v1.2.3 (abc1234)");
  });

  it("renders only the package version when commit id is unavailable", () => {
    siteStore().siteInfo = {
      mode: "debug",
      name: "Pkgly",
      description: "Repository server",
      is_installed: true,
      version: "1.2.3",
      commit_id: null,
    };

    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.get(".adminNav__version").text()).toBe("Pkgly v1.2.3");
  });
});
