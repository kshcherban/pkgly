import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

vi.mock("@/http", () => ({
  default: {
    get: vi.fn(),
    delete: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock("@/stores/session", () => ({
  sessionStore: () => ({
    user: { id: "user-1" },
  }),
}));

const tokenResponse = [
  {
    token: {
      id: 1,
      name: "CI token",
      source: "Manual",
      active: true,
      created_at: "2025-11-01T12:34:56Z",
    },
    scopes: [],
    repository_scopes: [],
  },
  {
    token: {
      id: 2,
      name: null,
      source: "docker_bearer",
      active: true,
      created_at: "2025-11-09T00:00:00Z",
    },
    scopes: [],
    repository_scopes: [],
  },
];

const vuetifyStubs = {
  "v-container": defineComponent({
    template: "<div data-testid='profile-tokens-container'><slot /></div>",
  }),
  "v-card": defineComponent({
    template: "<div class='v-card'><slot /></div>",
  }),
  "v-card-title": defineComponent({
    template: "<div class='v-card-title'><slot /></div>",
  }),
  "v-card-text": defineComponent({
    template: "<div class='v-card-text'><slot /></div>",
  }),
  "v-alert": defineComponent({
    props: { type: String },
    template: "<div data-testid='profile-tokens-error'><slot /></div>",
  }),
  "v-progress-circular": defineComponent({
    template: "<div data-testid='profile-tokens-loading'></div>",
  }),
  "v-expansion-panels": defineComponent({
    template: "<div class='v-expansion-panels'><slot /></div>",
  }),
  "v-expansion-panel": defineComponent({
    template: "<div class='v-expansion-panel'><slot /></div>",
  }),
  "v-expansion-panel-title": defineComponent({
    template: "<button class='v-expansion-panel-title' @click=\"$emit('click')\"><slot /></button>",
  }),
  "v-expansion-panel-text": defineComponent({
    template: "<div class='v-expansion-panel-text'><slot /></div>",
  }),
  "v-btn": defineComponent({
    props: { color: String, variant: String },
    emits: ["click"],
    template: "<button class='v-btn' @click=\"$emit('click', $event)\"><slot /></button>",
  }),
  "v-chip": defineComponent({
    props: { color: String, variant: String },
    template: "<span class='v-chip'><slot /></span>",
  }),
};

describe("ProfileTokens.vue", () => {
  it("renders tokens inside expansion panels with delete actions", async () => {
    const http = await import("@/http");
    (http.default.get as vi.Mock).mockResolvedValue({ data: tokenResponse });
    const module = await import("@/views/profile/ProfileTokens.vue");
    const ProfileTokens = module.default;

    const wrapper = mount(ProfileTokens, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.findAll(".v-expansion-panel")).toHaveLength(1);
    await wrapper.get('[data-testid="token-delete-button"]').trigger("click");
    expect(http.default.delete).toHaveBeenCalledWith("/api/user/token/delete/1");
  });
});
