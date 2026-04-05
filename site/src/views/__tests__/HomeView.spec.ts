import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, nextTick } from "vue";
import HomeView from "@/views/HomeView.vue";

const routerPush = vi.fn();

vi.mock("vue-router", () => ({
  useRouter: () => ({
    push: routerPush,
  }),
}));

const repositoriesMock = vi.fn().mockResolvedValue([
  {
    id: "repo-1",
    name: "Alpha",
    repository_type: "npm",
    storage_name: "Primary",
    auth_enabled: true,
    storage_usage_bytes: 0,
    active: true,
  },
  {
    id: "repo-2",
    name: "Bravo",
    repository_type: "maven",
    storage_name: "Secondary",
    auth_enabled: false,
    storage_usage_bytes: 1024,
    active: false,
  },
]);

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositories: repositoriesMock,
  }),
}));

vi.mock("@/stores/session", () => ({
  sessionStore: () => ({
    user: { admin: true, name: "Commander Shepard" },
  }),
}));

const VContainerStub = defineComponent({
  template: "<div class='v-container'><slot /></div>",
});

const VRowStub = defineComponent({
  template: "<div class='v-row'><slot /></div>",
});

const VColStub = defineComponent({
  template: "<div class='v-col'><slot /></div>",
});

const VAvatarStub = defineComponent({
  template: "<div class='v-avatar'><slot /></div>",
});

const HelmIconStub = defineComponent({
  template: "<span class='helm-icon-stub'></span>",
});

const DockerIconStub = defineComponent({
  template: "<span class='docker-icon-stub'></span>",
});

const VBtnStub = defineComponent({
  props: {
    to: [String, Object],
  },
  template: "<button class='v-btn'><slot /></button>",
});

const VCardStub = defineComponent({
  template: "<div class='v-card'><slot /></div>",
});

const VCardTitleStub = defineComponent({
  template: "<div class='v-card-title'><slot /></div>",
});

const VCardTextStub = defineComponent({
  template: "<div class='v-card-text'><slot /></div>",
});

const VCardActionsStub = defineComponent({
  template: "<div class='v-card-actions'><slot /></div>",
});

const VChipStub = defineComponent({
  template: "<span class='v-chip'><slot /></span>",
});

const VSpacerStub = defineComponent({
  template: "<span class='v-spacer'></span>",
});

const VIconStub = defineComponent({
  props: {
    icon: {
      type: String,
      default: "",
    },
    color: {
      type: String,
      default: "",
    },
  },
  template: "<i class='v-icon'><slot /></i>",
});

const VProgressCircularStub = defineComponent({
  template: "<div class='v-progress-circular'><slot /></div>",
});

const VAlertStub = defineComponent({
  template: "<div class='v-alert'><slot /></div>",
});

const VTextFieldStub = defineComponent({
  inheritAttrs: false,
  props: {
    modelValue: {
      type: String,
      default: "",
    },
    placeholder: {
      type: String,
      default: "",
    },
    clearable: {
      type: Boolean,
      default: false,
    },
  },
  emits: ["update:modelValue", "click:clear"],
  setup(props, { emit, slots, attrs }) {
    const onInput = (event: Event) => {
      emit("update:modelValue", (event.target as HTMLInputElement).value);
    };
    const dataTestid = (attrs["data-testid"] as string) ?? "v-text-field-input";
    return { props, slots, onInput, dataTestid };
  },
  template: `
    <label class="v-text-field">
      <input
        :value="props.modelValue"
        :placeholder="props.placeholder"
        :data-testid="dataTestid"
        @input="onInput" />
      <button
        type="button"
        class="v-text-field__clear"
        @click="$emit('click:clear')">
        clear
      </button>
      <slot />
    </label>
  `,
});

const vuetifyStubs = {
  "v-container": VContainerStub,
  "v-row": VRowStub,
  "v-col": VColStub,
  "v-avatar": VAvatarStub,
  "v-btn": VBtnStub,
  "v-card": VCardStub,
  "v-card-title": VCardTitleStub,
  "v-card-text": VCardTextStub,
  "v-card-actions": VCardActionsStub,
  "v-chip": VChipStub,
  "v-spacer": VSpacerStub,
  "v-icon": VIconStub,
  "v-progress-circular": VProgressCircularStub,
  "v-alert": VAlertStub,
  "v-text-field": VTextFieldStub,
  HelmIcon: HelmIconStub,
  DockerIcon: DockerIconStub,
};

describe("HomeView.vue", () => {
  it("opens repository cards on the repository page route", async () => {
    routerPush.mockReset();

    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    await wrapper.get(".repository-card").trigger("click");

    expect(routerPush).toHaveBeenCalledWith({
      name: "repository_page_by_name",
      params: {
        storageName: "Primary",
        repositoryName: "Alpha",
      },
    });
  });

  it("does not render welcome banner for authenticated user", async () => {
    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.text()).not.toContain("Welcome back, Commander Shepard");
    expect(wrapper.text()).not.toContain("Secure Artifacts");
  });

  it("exposes advanced package search help", async () => {
    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const input = wrapper.get('input[data-testid="repository-search-input"]');
    expect(input.attributes("placeholder")).toBe("Search packages or repositories");

    const helpButton = wrapper.get('[data-testid="search-help-button"]');
    await helpButton.trigger("click");

    expect(wrapper.find('[data-testid="search-help-modal"]').exists()).toBe(true);
  });

  it("provides a clearable repository search input", async () => {
    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const field = wrapper.getComponent(VTextFieldStub);
    expect(field.props("clearable")).toBe(true);

    field.vm.$emit("update:modelValue", "alp");
    await nextTick();
    expect((wrapper.vm as any).searchValue).toBe("alp");

    field.vm.$emit("click:clear");
    await nextTick();
    expect((wrapper.vm as any).searchValue).toBe("");
  });

  it("does not render repository title and catalog intro block", async () => {
    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.find(".repository-search-header__title").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Repository Catalog");
    expect(wrapper.text()).not.toContain(
      "Review repository status, confirm authentication posture, and drill into details.",
    );
  });

  it("uses the ruby language icon for ruby repositories", async () => {
    repositoriesMock.mockResolvedValueOnce([
      {
        id: "repo-3",
        name: "Ruby Gems",
        repository_type: "ruby",
        storage_name: "Primary",
        auth_enabled: true,
        storage_usage_bytes: 0,
        active: true,
      },
    ]);

    const wrapper = mount(HomeView, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const icons = wrapper.findAllComponents(VIconStub);
    const rubyIcon = icons.find((icon) => icon.props("icon") === "mdi-language-ruby");
    expect(rubyIcon).toBeTruthy();
    expect(rubyIcon?.props("color")).toBe("#CC342D");

    expect(icons.some((icon) => icon.props("icon") === "mdi-package-variant")).toBe(false);
  });
});
