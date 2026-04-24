import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
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
  it("renders section navigation without duplicate create actions", () => {
    const wrapper = mount(AdminNav, {
      global: { stubs },
    });

    expect(wrapper.text()).toContain("Users");
    expect(wrapper.text()).toContain("Storages");
    expect(wrapper.text()).toContain("Repositories");
    expect(wrapper.text()).toContain("System");
    expect(wrapper.text()).not.toContain("Create User");
    expect(wrapper.text()).not.toContain("Create Storage");
    expect(wrapper.text()).not.toContain("Create Repository");
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
});
