import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import ProfileSideNav from "@/components/nav/ProfileSideNav.vue";

let currentRoute = {
  name: "profileTokens",
  meta: {
    tag: "profileTokens",
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
    template: "<a :href='to' :data-active='$attrs[\"data-active\"]'><slot /></a>",
  }),
  FontAwesomeIcon: defineComponent({
    template: "<span />",
  }),
};

describe("ProfileSideNav.vue", () => {
  beforeEach(() => {
    currentRoute = {
      name: "profileTokens",
      meta: {
        tag: "profileTokens",
      },
    };
  });

  it("renders section navigation without a duplicate create token action", () => {
    const wrapper = mount(ProfileSideNav, {
      global: { stubs },
    });

    expect(wrapper.text()).toContain("Profile");
    expect(wrapper.text()).toContain("Login");
    expect(wrapper.text()).toContain("Tokens");
    expect(wrapper.text()).not.toContain("Create Token");
  });

  it("keeps tokens active on the create token route", () => {
    currentRoute = {
      name: "profileTokenCreate",
      meta: {
        tag: "profileTokens",
      },
    };

    const wrapper = mount(ProfileSideNav, {
      global: { stubs },
    });

    const tokensLink = wrapper.get('a[href="/profile/tokens"]');
    expect(tokensLink.attributes("data-active")).toBe("true");
  });

  it("activates only profile on profile route", () => {
    currentRoute = {
      name: "profile",
      meta: {},
    };

    const wrapper = mount(ProfileSideNav, {
      global: { stubs },
    });

    expect(wrapper.get('a[href="/profile"]').attributes("data-active")).toBe("true");
    expect(wrapper.get('a[href="/profile/login"]').attributes("data-active")).toBe("false");
    expect(wrapper.get('a[href="/profile/tokens"]').attributes("data-active")).toBe("false");
  });

  it("activates only login on login route", () => {
    currentRoute = {
      name: "profileLogin",
      meta: {},
    };

    const wrapper = mount(ProfileSideNav, {
      global: { stubs },
    });

    expect(wrapper.get('a[href="/profile"]').attributes("data-active")).toBe("false");
    expect(wrapper.get('a[href="/profile/login"]').attributes("data-active")).toBe("true");
    expect(wrapper.get('a[href="/profile/tokens"]').attributes("data-active")).toBe("false");
  });
});
