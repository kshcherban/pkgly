import { flushPromises, mount } from "@vue/test-utils";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositoryFromCache: (id: string) => ({
      id,
      name: id === "repo-1" ? "Repository One" : "Repository Two",
    }),
  }),
}));

const localStorageMock = {
  getItem: vi.fn().mockReturnValue(null),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
};

vi.stubGlobal("localStorage", localStorageMock);

let RepositoryToActionList: any;

beforeAll(async () => {
  RepositoryToActionList = (await import("@/components/nr/repository/RepositoryToActionList.vue")).default;
});

const SwitchInputStub = defineComponent({
  props: {
    modelValue: Boolean,
    id: String,
  },
  emits: ["update:modelValue"],
  template: `
    <label class="switch-input-stub">
      <input
        type="checkbox"
        :checked="modelValue"
        @change="$emit('update:modelValue', $event.target.checked)" />
      <slot />
    </label>
  `,
});

const RepositoryDropdownStub = defineComponent({
  props: {
    modelValue: {
      type: String,
      default: "",
    },
  },
  emits: ["update:modelValue"],
  setup(_, { emit }) {
    const onChange = (event: Event) => {
      const target = event.target as HTMLSelectElement;
      emit("update:modelValue", target.value);
    };
    return { onChange };
  },
  template: `
    <select
      class="repository-dropdown-stub"
      :value="modelValue"
      @change="onChange">
      <option value="">Select</option>
      <option value="repo-2">Repository Two</option>
    </select>
  `,
});

describe("RepositoryToActionList", () => {
  it("uses aligned, label-free headers for scope toggles", async () => {
    const wrapper = mount(RepositoryToActionList, {
      props: {
        modelValue: [
          {
            repositoryId: "repo-1",
            actions: {
              can_read: true,
              can_write: false,
              can_edit: true,
              asArray: () => ["read", "edit"],
            },
          },
        ],
      },
      global: {
        stubs: {
          SwitchInput: SwitchInputStub,
          RepositoryDropdown: RepositoryDropdownStub,
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();

    const headerCols = wrapper.findAll("#header .col");
    expect(headerCols[1].text().trim()).toBe("");
    expect(headerCols[2].text().trim()).toBe("");
    expect(headerCols[3].text().trim()).toBe("");
  });

  it("styles destructive remove buttons distinctly and keeps add width aligned", async () => {
    const wrapper = mount(RepositoryToActionList, {
      props: {
        modelValue: [
          {
            repositoryId: "repo-1",
            actions: {
              can_read: true,
              can_write: false,
              can_edit: true,
              asArray: () => ["read", "edit"],
            },
          },
          {
            repositoryId: "repo-2",
            actions: {
              can_read: true,
              can_write: true,
              can_edit: false,
              asArray: () => ["read", "write"],
            },
          },
        ],
      },
      global: {
        stubs: {
          SwitchInput: SwitchInputStub,
          RepositoryDropdown: RepositoryDropdownStub,
        },
        directives: {
          "auto-animate": () => undefined,
        },
      },
    });

    await flushPromises();

    const removeButtons = wrapper.findAll("button.actionButton--danger");
    expect(removeButtons).toHaveLength(2);
    removeButtons.forEach((btn) => {
      expect(btn.text()).toBe("Remove");
    });

    const addButton = wrapper.find("#create .actionButton--primary");
    expect(addButton.exists()).toBe(true);
    expect(addButton.text()).toBe("Add");
    expect(addButton.classes()).toContain("actionButton--fixed-width");

    removeButtons.forEach((btn) => {
      expect(btn.classes()).toContain("actionButton--fixed-width");
    });
  });
});
