import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

const getScopesMock = vi.fn();
const getRepositoriesMock = vi.fn();
const getRepositoryFromCacheMock = vi.fn();

vi.mock("@/stores/site", () => ({
  siteStore: () => ({
    getScopes: getScopesMock,
  }),
}));

vi.mock("@/stores/repositories", () => ({
  useRepositoryStore: () => ({
    getRepositories: getRepositoriesMock,
    getRepositoryFromCache: getRepositoryFromCacheMock,
  }),
}));

const tokenResponse = [
  {
    token: {
      id: 1,
      name: "CI token",
      description: "Build pipeline",
      source: "Manual",
      active: true,
      created_at: "2025-11-01T12:34:56Z",
      expires_at: "2099-01-01T00:00:00Z",
    },
    scopes: [{ id: 10, user_auth_token_id: 1, scope: "ReadRepository" }],
    repository_scopes: [
      {
        id: 20,
        user_auth_token_id: 1,
        repository_id: "repo-1",
        actions: ["Read", "Write"],
      },
      {
        id: 21,
        user_auth_token_id: 1,
        repository_id: "repo-missing",
        actions: ["Edit"],
      },
    ],
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
  {
    token: {
      id: 3,
      name: "Old token",
      source: "Manual",
      active: true,
      created_at: "2024-01-01T00:00:00Z",
      expires_at: "2024-02-01T00:00:00Z",
    },
    scopes: [],
    repository_scopes: [],
  },
  {
    token: {
      id: 4,
      name: "Revoked token",
      source: "Manual",
      active: false,
      created_at: "2025-01-01T00:00:00Z",
      expires_at: null,
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
  "v-btn": defineComponent({
    props: { color: String, variant: String, to: [String, Object] },
    emits: ["click"],
    template: `
      <button
        class='v-btn'
        :data-to-name="to && typeof to === 'object' ? to.name : to"
        @click="$emit('click', $event)">
        <slot />
      </button>
    `,
  }),
  "v-chip": defineComponent({
    props: { color: String, variant: String },
    template: "<span class='v-chip'><slot /></span>",
  }),
};

describe("ProfileTokens.vue", () => {
  beforeEach(async () => {
    const http = await import("@/http");
    vi.mocked(http.default.get).mockReset();
    vi.mocked(http.default.delete).mockReset();
    vi.mocked(http.default.delete).mockResolvedValue(undefined);
    getScopesMock.mockReset();
    getScopesMock.mockResolvedValue([
      {
        key: "ReadRepository",
        name: "Read Repository",
        description: "Can read all repositories",
      },
    ]);
    getRepositoriesMock.mockReset();
    getRepositoriesMock.mockResolvedValue([{ id: "repo-1", name: "local-maven" }]);
    getRepositoryFromCacheMock.mockReset();
    getRepositoryFromCacheMock.mockImplementation((id: string) => {
      if (id === "repo-1") {
        return { id: "repo-1", name: "local-maven" };
      }
      return undefined;
    });
  });

  it("renders token rows with status, expiration, and scope details", async () => {
    const http = await import("@/http");
    vi.mocked(http.default.get).mockResolvedValue({ data: tokenResponse });
    const module = await import("@/views/profile/ProfileTokens.vue");
    const ProfileTokens = module.default;

    const wrapper = mount(ProfileTokens, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    expect(wrapper.findAll('[data-testid="token-row"]')).toHaveLength(3);
    expect(wrapper.findAll("button").filter((button) => button.text().includes("New Token"))).toHaveLength(1);
    expect(wrapper.text()).toContain("CI token");
    expect(wrapper.text()).toContain("Build pipeline");
    expect(wrapper.text()).toContain("Active");
    expect(wrapper.text()).toContain("Expired");
    expect(wrapper.text()).toContain("Revoked");
    expect(wrapper.text()).toContain("Read Repository");
    expect(wrapper.text()).toContain("local-maven");
    expect(wrapper.text()).toContain("Read, Write");
    expect(wrapper.text()).toContain("repo-missing");
    expect(wrapper.text()).toContain("Edit");
    expect(wrapper.text()).toContain("Never expires");
    expect(wrapper.text()).not.toContain("docker_bearer");
    await wrapper.get('[data-testid="token-delete-button"]').trigger("click");
    expect(http.default.delete).toHaveBeenCalledWith("/api/user/token/delete/1");
  });

  it("shows one create token CTA when no tokens exist", async () => {
    const http = await import("@/http");
    vi.mocked(http.default.get).mockResolvedValue({ data: [] });
    const module = await import("@/views/profile/ProfileTokens.vue");
    const ProfileTokens = module.default;

    const wrapper = mount(ProfileTokens, {
      global: {
        stubs: vuetifyStubs,
      },
    });

    await flushPromises();

    const createTokenButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().includes("Create Token"));
    expect(wrapper.text()).toContain("No tokens yet");
    expect(wrapper.text()).not.toContain("New Token");
    expect(createTokenButtons).toHaveLength(1);
    expect(createTokenButtons[0]!.attributes("data-to-name")).toBe("profileTokenCreate");
  });
});
