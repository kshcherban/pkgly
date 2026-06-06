// ABOUTME: Verifies search-help recipes, dialog semantics, and keyboard focus behavior.
// ABOUTME: Covers the custom search dialog independently from the home page.
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { describe, expect, it } from "vitest";
import RepositorySearchHeader from "@/components/nr/repository/RepositorySearchHeader.vue";

const textFieldStub = {
  props: ["modelValue"],
  emits: ["update:modelValue", "click:clear"],
  template: "<input :value='modelValue' aria-label='Search repositories or packages' />",
};

function mountHeader() {
  return mount(RepositorySearchHeader, {
    attachTo: document.body,
    props: {
      modelValue: "",
    },
    global: {
      stubs: {
        "v-text-field": textFieldStub,
        "v-icon": {
          template: "<span><slot /></span>",
        },
      },
    },
  });
}

describe("RepositorySearchHeader.vue", () => {
  it("opens an accessible dialog and moves focus to its close control", async () => {
    const wrapper = mountHeader();
    const help = wrapper.get('[data-testid="search-help-button"]');

    expect(help.attributes("aria-expanded")).toBe("false");
    await help.trigger("click");
    await nextTick();

    const dialog = wrapper.get('[data-testid="search-help-modal"]');
    expect(help.attributes("aria-expanded")).toBe("true");
    expect(dialog.attributes("role")).toBe("dialog");
    expect(dialog.attributes("aria-labelledby")).toBe("searchHelpTitle");
    expect(document.activeElement).toBe(
      wrapper.get('[aria-label="Close search help"]').element,
    );
    wrapper.unmount();
  });

  it.each([
    ["package", "package:express"],
    ["repository", "repo:npm-hosted"],
    ["type", "type:helm"],
    ["version", "version:>=1.0.0"],
    ["combined", "package:express version:>=4.0.0 type:npm"],
  ])("applies the %s recipe and closes the dialog", async (recipe, query) => {
    const wrapper = mountHeader();
    await wrapper.get('[data-testid="search-help-button"]').trigger("click");
    await wrapper.get(`[data-testid="search-recipe-${recipe}"]`).trigger("click");

    expect(wrapper.emitted("update:modelValue")?.at(-1)).toEqual([query]);
    expect(wrapper.find('[data-testid="search-help-modal"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("closes on Escape and returns focus to the help button", async () => {
    const wrapper = mountHeader();
    const help = wrapper.get('[data-testid="search-help-button"]');
    await help.trigger("click");
    await nextTick();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await nextTick();

    expect(wrapper.find('[data-testid="search-help-modal"]').exists()).toBe(false);
    expect(document.activeElement).toBe(help.element);
    wrapper.unmount();
  });

  it("closes from the close button and overlay", async () => {
    const wrapper = mountHeader();
    const help = wrapper.get('[data-testid="search-help-button"]');

    await help.trigger("click");
    await wrapper.get('[aria-label="Close search help"]').trigger("click");
    expect(wrapper.find('[data-testid="search-help-modal"]').exists()).toBe(false);

    await help.trigger("click");
    await wrapper.get('[data-testid="search-help-modal"]').trigger("click");
    expect(wrapper.find('[data-testid="search-help-modal"]').exists()).toBe(false);
    wrapper.unmount();
  });
});
