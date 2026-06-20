// ABOUTME: Tests BrandMark renders the logo, wordmark, home link, and layout variants.
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { defineComponent } from "vue";
import BrandMark from "@/components/layout/BrandMark.vue";

const RouterLinkStub = defineComponent({
  name: "RouterLink",
  props: { to: { type: [String, Object], default: "/" } },
  computed: {
    href(): string {
      return typeof this.to === "string" ? this.to : (this.to as any).path || "/";
    },
  },
  template: '<a :href="href"><slot /></a>',
});

const VAvatarStub = defineComponent({
  name: "VAvatar",
  props: { image: String, size: [Number, String] },
  template: '<span class="v-avatar" :data-image="image" :data-size="size"></span>',
});

function mountBrand(props: Record<string, unknown>) {
  return mount(BrandMark, {
    props,
    global: {
      stubs: { RouterLink: RouterLinkStub, "v-avatar": VAvatarStub },
    },
  });
}

describe("BrandMark", () => {
  it("renders the Pkgly wordmark and the logo image", () => {
    const wrapper = mountBrand({});
    expect(wrapper.text()).toContain("Pkgly");
    const avatar = wrapper.find(".v-avatar");
    expect(avatar.exists()).toBe(true);
    expect(avatar.attributes("data-image")).toBe("/logo.svg");
  });

  it("links to the home route", () => {
    const wrapper = mountBrand({});
    expect(wrapper.find("a").exists()).toBe(true);
  });

  it("defaults to a small horizontal layout for the app bar", () => {
    const wrapper = mountBrand({});
    expect(wrapper.classes()).toContain("brand-mark");
    expect(wrapper.find(".v-avatar").attributes("data-size")).toBe("40");
  });

  it("supports a stacked large layout for the login screen", () => {
    const wrapper = mountBrand({ stacked: true, size: 64, wordmarkClass: "text-h4" });
    expect(wrapper.classes()).toContain("brand-mark--stacked");
    expect(wrapper.find(".v-avatar").attributes("data-size")).toBe("64");
    expect(wrapper.find(".brand-mark__wordmark").classes()).toContain("text-h4");
  });
});
