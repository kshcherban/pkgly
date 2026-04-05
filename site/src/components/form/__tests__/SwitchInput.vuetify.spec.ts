import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it } from "vitest";
import { defineComponent, nextTick, ref } from "vue";
import { createVuetify } from "vuetify";
import * as components from "vuetify/components";
import * as directives from "vuetify/directives";

import SwitchInput from "../SwitchInput.vue";

function ensureDomPolyfills() {
  if (typeof (globalThis as any).ResizeObserver === "undefined") {
    (globalThis as any).ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }

  if (typeof window !== "undefined" && typeof window.matchMedia === "undefined") {
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    })) as any;
  }
}

describe("SwitchInput.vue (vuetify)", () => {
  beforeEach(() => {
    ensureDomPolyfills();
  });

  it("toggles via click when using Vuetify v-switch", async () => {
    const vuetify = createVuetify({ components, directives });

    const Host = defineComponent({
      components: { SwitchInput },
      setup() {
        const enabled = ref(false);
        return { enabled };
      },
      template: `<SwitchInput id="switch-test" v-model="enabled">Enable</SwitchInput>`,
    });

    const wrapper = mount(Host, {
      global: {
        plugins: [vuetify],
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    const checkbox = wrapper.find("input#switch-test");
    expect(checkbox.exists()).toBe(true);

    await checkbox.trigger("click");
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });
});

