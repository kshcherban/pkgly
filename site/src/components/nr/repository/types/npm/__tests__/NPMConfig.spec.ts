import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import NPMConfig from "../NPMConfig.vue";

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositories: vi.fn().mockResolvedValue([
      {
        id: "npm-hosted",
        name: "npm-hosted",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "npm",
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
        id: "npm-proxy",
        name: "npm-proxy",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "npm",
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

describe("NPMConfig virtual repositories", () => {
  beforeEach(() => {
    httpMock.get.mockReset();
    httpMock.post.mockReset();
    httpMock.put.mockReset();
    httpMock.get.mockResolvedValue({ data: { type: "Hosted" } });
  });

  it("adds virtual members and binds publish target", async () => {
    const wrapper = mount(NPMConfig, {
      props: { settingName: "npm" },
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

    // publish target should stay unset until user chooses
    expect(vm.publishTarget).toBe("");
    expect(vm.virtualConfigSafe.publish_to).toBeNull();

    const publishOptions = vm.publishTargetOptions;
    expect(publishOptions.some((option: any) => option.value === "npm-hosted")).toBe(true);
  });

  it("marks proxy remove buttons as full-width actions", async () => {
    const wrapper = mount(NPMConfig, {
      props: { settingName: "npm" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(controlStubs.DropDown);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const removeBtn = wrapper.get(".route-row button");
    expect(removeBtn.classes()).toContain("route-action");
  });

  it("keeps Add Route button label consistent", async () => {
    const wrapper = mount(NPMConfig, {
      props: { settingName: "npm" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    // switch to Proxy so routes section renders
    const typeSelect = wrapper.findComponent(controlStubs.DropDown);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const addBtn = wrapper.findAll("button").find((btn) => btn.text().includes("Add Route"));
    expect(addBtn?.text()).not.toBeUndefined();
  });

  it("saves updated virtual members for existing repository", async () => {
    httpMock.get.mockImplementation(async (url: string) => {
      if (url.endsWith("/virtual/members")) {
        return {
          data: {
            members: [
              {
                repository_id: "npm-hosted",
                repository_name: "npm-hosted",
                priority: 0,
                enabled: true,
              },
            ],
            resolution_order: "Priority",
            cache_ttl_seconds: 60,
            publish_to: null,
          },
        };
      }
      return { data: { type: "Hosted" } };
    });

    const wrapper = mount(NPMConfig, {
      props: { settingName: "npm", repository: "virtual-1" },
      global: { stubs: controlStubs },
    });

    await flushPromises();

    const vm: any = wrapper.vm;
    expect(vm.virtualMembers.length).toBe(1);
    // mutate priority to ensure change is captured
    vm.virtualMembers[0].priority = 5;

    await wrapper.find("form").trigger("submit.prevent");
    await flushPromises();

    expect(httpMock.post).toHaveBeenCalledWith(
      "/api/repository/virtual-1/virtual/members",
      expect.objectContaining({
        members: [
          expect.objectContaining({
            repository_id: "npm-hosted",
            priority: 5,
            enabled: true,
          }),
        ],
        publish_to: null,
        cache_ttl_seconds: 60,
      }),
    );
  });
});
