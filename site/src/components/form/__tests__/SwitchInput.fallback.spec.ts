import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick, ref } from "vue";

import SwitchInput from "../SwitchInput.vue";

const VSwitchStub = defineComponent({
  props: ["id", "modelValue"],
  emits: ["update:modelValue"],
  template: `
    <div class="v-switch">
      <div class="v-selection-control__input">
        <input :id="id" type="checkbox" :checked="modelValue" />
        <div class="thumb" data-testid="thumb"></div>
      </div>
      <div class="v-label">
        <slot name="label" />
      </div>
    </div>
  `,
});

describe("SwitchInput.vue (fallback click handling)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("toggles when clicking the switch control even if v-switch does not emit update", async () => {
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
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find("[data-testid='thumb']").trigger("click");
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });

  it("toggles when clicking the label text", async () => {
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
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find(".switch-label-text").trigger("click");
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });

  it("toggles when clicking the wrapper background even if v-switch does not emit update", async () => {
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
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find(".switch-wrapper").trigger("click");
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });

  it("syncs v-model to the native input when the input toggles but v-switch does not emit update", async () => {
    const NativeToggleNoEmitSwitchStub = defineComponent({
      props: ["id", "modelValue"],
      setup() {
        const onClick = (event: MouseEvent) => {
          event.preventDefault();
          const target = event.target;
          if (target instanceof HTMLInputElement) {
            target.checked = !target.checked;
          }
        };
        return { onClick };
      },
      template: `
        <div class="v-switch">
          <div class="v-selection-control__input">
            <input :id="id" type="checkbox" :checked="modelValue" @click="onClick" />
          </div>
          <div class="v-label">
            <slot name="label" />
          </div>
        </div>
      `,
    });

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
        stubs: {
          "v-switch": NativeToggleNoEmitSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find("input#switch-test").trigger("click");
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });

  it("does not double-toggle when the native input toggles but v-switch emits model update later", async () => {
    const DelayedEmitSwitchStub = defineComponent({
      props: ["id", "modelValue"],
      emits: ["update:modelValue"],
      setup(props, { emit }) {
        const onClick = (event: MouseEvent) => {
          const target = event.target;
          if (target instanceof HTMLInputElement) {
            target.checked = !target.checked;
          }
          setTimeout(() => {
            emit("update:modelValue", !props.modelValue);
          }, 10);
        };
        return { onClick };
      },
      template: `
        <div class="v-switch">
          <div class="v-selection-control__input">
            <input :id="id" type="checkbox" :checked="modelValue" @click="onClick" />
          </div>
          <div class="v-label">
            <slot name="label" />
          </div>
        </div>
      `,
    });

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
        stubs: {
          "v-switch": DelayedEmitSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find("input#switch-test").trigger("click");
    // Run the wrapper fallback tick (0ms) and the delayed model emit (10ms).
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(true);
  });

  it("does not toggle when clicking an interactive element inside the label", async () => {
    const Host = defineComponent({
      components: { SwitchInput },
      setup() {
        const enabled = ref(false);
        return { enabled };
      },
      template: `
        <SwitchInput id="switch-test" v-model="enabled">
          Enable
          <template #comment>
            <a href="#" data-testid="label-link">Docs</a>
          </template>
        </SwitchInput>
      `,
    });

    const wrapper = mount(Host, {
      global: {
        stubs: {
          "v-switch": VSwitchStub,
        },
      },
      attachTo: document.body,
    });

    expect((wrapper.vm as any).enabled).toBe(false);

    await wrapper.find("[data-testid='label-link']").trigger("click");
    vi.runOnlyPendingTimers();
    await nextTick();

    expect((wrapper.vm as any).enabled).toBe(false);
  });
});
