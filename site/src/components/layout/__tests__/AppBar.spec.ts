import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import AppBar from "@/components/layout/AppBar.vue";

const VAppBarStub = {
  template: "<div class='v-app-bar'><slot /></div>",
};

const VContainerStub = {
  template: "<div class='v-container'><slot /></div>",
};

const VAvatarStub = {
  template: "<div class='v-avatar'><slot /></div>",
};

const VSpacerStub = {
  template: "<div class='v-spacer'></div>",
};

const VBtnStub = {
  props: {
    to: {
      type: [String, Object],
      default: undefined,
    },
    variant: {
      type: String,
      default: "",
    },
  },
  template: "<button class='v-btn'><slot /></button>",
};

const VMenuStub = {
  template: "<div class='v-menu'><slot /></div>",
};

const VListStub = {
  template: "<ul class='v-list'><slot /></ul>",
};

const VListItemStub = {
  template: "<li class='v-list-item'><slot /></li>",
};

const VListItemTitleStub = {
  template: "<span class='v-list-item-title'><slot /></span>",
};

const VDividerStub = {
  template: "<hr class='v-divider' />",
};

const VIconStub = {
  template: "<i class='v-icon'><slot /></i>",
};

describe("AppBar.vue", () => {
  it("renders Pkgly brand title", () => {
    const wrapper = mount(AppBar, {
      props: {
        user: undefined,
      },
      global: {
        stubs: {
          "router-link": {
            template: "<a class='router-link'><slot /></a>",
          },
          "v-app-bar": VAppBarStub,
          "v-container": VContainerStub,
          "v-avatar": VAvatarStub,
          "v-spacer": VSpacerStub,
          "v-btn": VBtnStub,
          "v-menu": VMenuStub,
          "v-list": VListStub,
          "v-list-item": VListItemStub,
          "v-list-item-title": VListItemTitleStub,
          "v-divider": VDividerStub,
          "v-icon": VIconStub,
        },
      },
    });

    expect(wrapper.text()).toContain("Pkgly");
    expect(wrapper.text()).not.toContain("Repository");
  });

  it("does not render browse repositories button when user is present", () => {
    const wrapper = mount(AppBar, {
      props: {
        user: {
          username: "chief",
        },
      },
      global: {
        stubs: {
          "router-link": {
            template: "<a class='router-link'><slot /></a>",
          },
          "v-app-bar": VAppBarStub,
          "v-container": VContainerStub,
          "v-avatar": VAvatarStub,
          "v-spacer": VSpacerStub,
          "v-btn": VBtnStub,
          "v-menu": VMenuStub,
          "v-list": VListStub,
          "v-list-item": VListItemStub,
          "v-list-item-title": VListItemTitleStub,
          "v-divider": VDividerStub,
          "v-icon": VIconStub,
        },
      },
    });

    const buttons = wrapper.findAll(".v-btn");
    const browseButton = buttons.find((btn) => btn.text().includes("Browse Repositories"));
    expect(browseButton).toBeUndefined();
  });

  it("does not render browse button when user is missing", () => {
    const wrapper = mount(AppBar, {
      props: {
        user: undefined,
      },
      global: {
        stubs: {
          "router-link": {
            template: "<a class='router-link'><slot /></a>",
          },
          "v-app-bar": VAppBarStub,
          "v-container": VContainerStub,
          "v-avatar": VAvatarStub,
          "v-spacer": VSpacerStub,
          "v-btn": VBtnStub,
          "v-menu": VMenuStub,
          "v-list": VListStub,
          "v-list-item": VListItemStub,
          "v-list-item-title": VListItemTitleStub,
          "v-divider": VDividerStub,
          "v-icon": VIconStub,
        },
      },
    });

    const buttons = wrapper.findAll(".v-btn");
    const browseButton = buttons.find((btn) => btn.text().includes("Browse Repositories"));
    expect(browseButton).toBeUndefined();
  });

  it("renders admin panel button in the top bar for admin users", () => {
    const wrapper = mount(AppBar, {
      props: {
        user: {
          username: "chief",
          admin: true,
        },
      },
      global: {
        stubs: {
          "router-link": {
            template: "<a class='router-link'><slot /></a>",
          },
          "v-app-bar": VAppBarStub,
          "v-container": VContainerStub,
          "v-avatar": VAvatarStub,
          "v-spacer": VSpacerStub,
          "v-btn": VBtnStub,
          "v-menu": VMenuStub,
          "v-list": VListStub,
          "v-list-item": VListItemStub,
          "v-list-item-title": VListItemTitleStub,
          "v-divider": VDividerStub,
          "v-icon": VIconStub,
        },
      },
    });

    const topBarButtons = wrapper.findAll(".v-btn");
    expect(topBarButtons.some((btn) => btn.text().includes("Admin Panel"))).toBe(true);
    expect(wrapper.find(".v-list").text()).not.toContain("Admin Panel");
  });

  it("does not render admin panel button for non-admin users", () => {
    const wrapper = mount(AppBar, {
      props: {
        user: {
          username: "chief",
          admin: false,
        },
      },
      global: {
        stubs: {
          "router-link": {
            template: "<a class='router-link'><slot /></a>",
          },
          "v-app-bar": VAppBarStub,
          "v-container": VContainerStub,
          "v-avatar": VAvatarStub,
          "v-spacer": VSpacerStub,
          "v-btn": VBtnStub,
          "v-menu": VMenuStub,
          "v-list": VListStub,
          "v-list-item": VListItemStub,
          "v-list-item-title": VListItemTitleStub,
          "v-divider": VDividerStub,
          "v-icon": VIconStub,
        },
      },
    });

    const topBarButtons = wrapper.findAll(".v-btn");
    expect(topBarButtons.some((btn) => btn.text().includes("Admin Panel"))).toBe(false);
  });
});
