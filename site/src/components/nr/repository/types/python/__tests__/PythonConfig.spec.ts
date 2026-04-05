import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";

const storageMock = {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
  clear: () => undefined,
};

(globalThis as any).localStorage = storageMock;
if (typeof window !== "undefined") {
  (window as any).localStorage = storageMock;
}

let PythonConfig: any;

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositories: vi.fn().mockResolvedValue([
      {
        id: "python-hosted",
        name: "python-hosted",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "python",
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
        id: "python-proxy",
        name: "python-proxy",
        storage_name: "test-storage",
        storage_id: "s1",
        repository_type: "python",
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

beforeEach(async () => {
  httpMock.get.mockReset();
  httpMock.post.mockReset();
  httpMock.put.mockReset();
  httpMock.get.mockResolvedValue({ data: { type: "Hosted" } });
  PythonConfig = (await import("../PythonConfig.vue")).default;
});

const DropDownStub = defineComponent({
  props: ["modelValue", "options"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const local = ref(props.modelValue ?? "");
    const onChange = (event: Event) => {
      const next = (event.target as HTMLSelectElement).value;
      local.value = next;
      emit("update:modelValue", next);
    };
    return { local, slots, onChange };
  },
  template: `
    <label class="dropdown-stub">
      <slot />
      <select :value="local" @change="onChange">
        <option v-for="option in options" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </select>
    </label>
  `,
});

const TextInputStub = defineComponent({
  props: ["modelValue", "placeholder", "id"],
  emits: ["update:modelValue"],
  setup(props, { emit, slots }) {
    const onInput = (event: Event) => {
      const target = event.target as HTMLInputElement | null;
      emit("update:modelValue", target?.value ?? "");
    };
    return { props, slots, onInput };
  },
  template: `
    <label class="text-input-stub">
      <slot />
      <input
        :id="id"
        :placeholder="placeholder"
        :value="modelValue"
        @input="onInput" />
    </label>
  `,
});

const SwitchInputStub = defineComponent({
  props: ["modelValue", "id"],
  emits: ["update:modelValue"],
  template: `
    <label class="switch-input-stub">
      <slot />
      <input type="checkbox" :checked="modelValue" @change="$emit('update:modelValue', $event.target && ($event.target).checked)" />
    </label>
  `,
});

const VBtnStub = defineComponent({
  inheritAttrs: false,
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
});

const controlStubs = {
  DropDown: DropDownStub,
  TextInput: TextInputStub,
  SwitchInput: SwitchInputStub,
  SubmitButton: defineComponent({ template: "<button type='submit'><slot /></button>" }),
  ProxyCacheNotice: defineComponent({ template: "<div />" }),
  "v-btn": VBtnStub,
};

describe("PythonConfig virtual repositories", () => {
  it("adds virtual members and exposes publish target options", async () => {
    const wrapper = mount(PythonConfig, {
      props: { settingName: "python" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(DropDownStub);
    await typeSelect.find("select").setValue("Virtual");
    await flushPromises();

    const addButton = wrapper.get('[data-testid="virtual-add-member"]');
    await addButton.trigger("click");
    await flushPromises();

    const vm: any = wrapper.vm;
    expect(vm.virtualMembers.length).toBe(1);
    expect(vm.publishTarget).toBe("");
    expect(vm.virtualConfigSafe.publish_to).toBeNull();

    const publishOptions = vm.publishTargetOptions;
    expect(publishOptions.some((option: any) => option.value === "python-hosted")).toBe(true);
  });

  it("saves updated virtual members for existing repository", async () => {
    httpMock.get.mockImplementation(async (url: string) => {
      if (url.endsWith("/virtual/members")) {
        return {
          data: {
            members: [
              {
                repository_id: "python-hosted",
                repository_name: "python-hosted",
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

    const wrapper = mount(PythonConfig, {
      props: { settingName: "python", repository: "virtual-1" },
      global: { stubs: controlStubs },
    });

    await flushPromises();

    const vm: any = wrapper.vm;
    expect(vm.virtualMembers.length).toBe(1);
    vm.virtualMembers[0].priority = 5;

    await wrapper.find("form").trigger("submit.prevent");
    await flushPromises();

    expect(httpMock.post).toHaveBeenCalledWith(
      "/api/repository/virtual-1/virtual/members",
      expect.objectContaining({
        members: [
          expect.objectContaining({
            repository_id: "python-hosted",
            priority: 5,
            enabled: true,
          }),
        ],
        publish_to: null,
        cache_ttl_seconds: 60,
      }),
    );
  });

  it("renders proxy remove buttons with the full-width action class", async () => {
    const wrapper = mount(PythonConfig, {
      props: { settingName: "python" },
      global: { stubs: controlStubs },
    });

    await flushPromises();
    const typeSelect = wrapper.findComponent(DropDownStub);
    await typeSelect.find("select").setValue("Proxy");
    await flushPromises();

    const removeBtn = wrapper.get(".route-row button");
    expect(removeBtn.classes()).toContain("route-action");
  });
});
