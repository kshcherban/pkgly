import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import NugetConfig from "../NugetConfig.vue";

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositories: vi.fn().mockResolvedValue([
      {
        id: "nuget-hosted",
        name: "nuget-hosted",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "nuget",
        repository_kind: "hosted",
        active: true,
        visibility: "Public",
        updated_at: "",
        created_at: "",
        auth_enabled: false,
        storage_usage_bytes: null,
        storage_usage_updated_at: null,
      },
      {
        id: "nuget-proxy",
        name: "nuget-proxy",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "nuget",
        repository_kind: "proxy",
        active: true,
        visibility: "Public",
        updated_at: "",
        created_at: "",
        auth_enabled: false,
        storage_usage_bytes: null,
        storage_usage_updated_at: null,
      },
    ]),
  }),
}));

const httpMock = vi.hoisted(() => ({
  get: vi.fn().mockResolvedValue({ data: { type: "Hosted" } }),
  put: vi.fn(),
  post: vi.fn(),
}));

vi.mock("@/http", () => ({
  default: httpMock,
}));

const controlStubs = {
  DropDown: defineComponent({
    props: ["modelValue", "options", "id"],
    emits: ["update:modelValue"],
    setup(props, { emit, slots }) {
      const local = ref(props.modelValue ?? "");
      const onChange = (event: Event) => {
        const next = (event.target as HTMLSelectElement).value;
        local.value = next;
        emit("update:modelValue", next);
      };
      return { props, slots, local, onChange };
    },
    template: `
      <label class="dropdown">
        <slot />
        <select :value="local" @change="onChange">
          <option v-for="option in props.options" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
      </label>
    `,
  }),
  TextInput: defineComponent({
    props: ["modelValue", "id", "type", "placeholder"],
    emits: ["update:modelValue"],
    template: `
      <label class="text-input">
        <slot />
        <input :id="id" :type="type" :placeholder="placeholder" :value="modelValue" @input="$emit('update:modelValue', $event.target && ($event.target).value)" />
      </label>
    `,
  }),
  SwitchInput: defineComponent({
    props: ["modelValue", "id"],
    emits: ["update:modelValue"],
    template: `
      <label class="switch-input">
        <slot />
        <input type="checkbox" :checked="modelValue" @change="$emit('update:modelValue', $event.target && ($event.target).checked)" />
      </label>
    `,
  }),
  SubmitButton: defineComponent({
    template: `<button type="submit"><slot /></button>`,
  }),
  ProxyCacheNotice: defineComponent({
    template: `<div />`,
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: ["type", "color", "variant", "prependIcon", "disabled"],
    emits: ["click"],
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
  }),
};

describe("NugetConfig", () => {
  it("switches to proxy mode with the default upstream index", async () => {
    const wrapper = mount(NugetConfig, {
      props: { settingName: "nuget" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(controlStubs.DropDown);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const vm: any = wrapper.vm;
    expect(vm.proxyConfig.upstream_url).toBe("https://api.nuget.org/v3/index.json");
    expect(vm.value.type).toBe("Proxy");
  });

  it("adds virtual members and exposes hosted publish targets", async () => {
    const wrapper = mount(NugetConfig, {
      props: { settingName: "nuget" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(controlStubs.DropDown);
    await typeSelect.find("select").setValue("Virtual");
    await flushPromises();

    const addButton = wrapper.get('[data-testid="virtual-add-member"]');
    await addButton.trigger("click");
    await flushPromises();

    const vm: any = wrapper.vm;
    expect(vm.virtualMembers.length).toBe(1);
    expect(vm.publishTargetOptions.some((option: any) => option.value === "nuget-hosted")).toBe(true);
  });
});
