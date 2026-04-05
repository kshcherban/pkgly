import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import type { MavenProxyConfigType } from "@/components/nr/repository/types/maven/maven";

vi.mock("@vue/devtools-kit", () => ({}));
vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => ({
    error: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
  }),
}));

const storageMock = {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
  clear: () => undefined,
};

vi.stubGlobal("localStorage", storageMock as any);
if (typeof window !== "undefined") {
  (window as any).localStorage = storageMock;
}

const MavenProxyConfig = (await import("@/components/nr/repository/types/maven/MavenProxyConfig.vue")).default;

const TextInputStub = defineComponent({
  name: "TextInputStub",
  props: {
    modelValue: {
      type: String,
      default: "",
    },
  },
  emits: ["update:modelValue"],
  setup(props, { emit, attrs }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { attrs, onInput };
  },
  template: `
    <input
      class="text-input-stub"
      :value="modelValue"
      v-bind="attrs"
      @input="onInput" />
  `,
});

const VRowStub = defineComponent({
  template: `<div class="v-row-stub"><slot /></div>`,
});
const VColStub = defineComponent({
  template: `<div class="v-col-stub"><slot /></div>`,
});
const VBtnStub = defineComponent({
  emits: ["click"],
  inheritAttrs: false,
  setup(_, { emit, attrs, slots }) {
    return () =>
      h(
        "button",
        {
          ...attrs,
          type: (attrs.type as string) || "button",
          disabled: attrs.disabled as boolean | undefined,
          onClick: () => emit("click"),
        },
        slots.default?.(),
      );
  },
});

function createHarness() {
  return defineComponent({
    components: { MavenProxyConfig },
    setup() {
      const state = ref<MavenProxyConfigType>({ routes: [] });
      return { state };
    },
    template: `<MavenProxyConfig v-model="state" />`,
  });
}

describe("MavenProxyConfig.vue", () => {
  it("prefills the first upstream route with Maven Central", async () => {
    const Harness = createHarness();
    const wrapper = mount(Harness, {
      global: {
        stubs: {
          TextInput: TextInputStub,
          "v-row": VRowStub,
          "v-col": VColStub,
          "v-btn": VBtnStub,
          "v-divider": defineComponent({
            template: `<div class="v-divider-stub"></div>`,
          }),
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();

    const current = (wrapper.vm as { state: MavenProxyConfigType }).state;
    expect(current.routes.length).toBeGreaterThan(0);
    expect(current.routes[0]?.url).toBe("https://repo1.maven.org/maven2/");
    expect(current.routes[0]?.name).toBe("Maven Central");
  });

  it("renders without divider separator", async () => {
    const Harness = createHarness();
    const wrapper = mount(Harness, {
      global: {
        stubs: {
          TextInput: TextInputStub,
          "v-row": VRowStub,
          "v-col": VColStub,
          "v-btn": VBtnStub,
          "v-divider": defineComponent({
            template: `<div class="v-divider-stub"></div>`,
          }),
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();

    expect(wrapper.find(".v-divider-stub").exists()).toBe(false);
  });

  it("applies the route action class to remove buttons", async () => {
    const Harness = createHarness();
    const wrapper = mount(Harness, {
      global: {
        stubs: {
          TextInput: TextInputStub,
          "v-row": VRowStub,
          "v-col": VColStub,
          "v-btn": VBtnStub,
          "v-divider": defineComponent({
            template: `<div class="v-divider-stub"></div>`,
          }),
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();
    const removeBtn = wrapper.get(".maven-proxy__route button");
    expect(removeBtn.classes()).toContain("route-action");
  });

  it("shows Add Route button below routes with correct label", async () => {
    const Harness = createHarness();
    const wrapper = mount(Harness, {
      global: {
        stubs: {
          TextInput: TextInputStub,
          "v-row": VRowStub,
          "v-col": VColStub,
          "v-btn": VBtnStub,
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();
    const addBtn = wrapper.get(".route-add");
    expect(addBtn.text()).toContain("Add Route");
  });
});
