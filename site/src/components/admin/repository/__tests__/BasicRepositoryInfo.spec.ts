import { describe, expect, it, vi, beforeEach } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

vi.mock("@/http", () => ({
  default: {
    delete: vi.fn(),
  },
}));

vi.mock("@/router", () => ({
  default: {
    push: vi.fn(),
  },
}));

const mockAlerts = {
  success: vi.fn(),
  error: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

import BasicRepositoryInfo from "../BasicRepositoryInfo.vue";

const vuetifyStubs = {
  "v-card": {
    template: `<div data-stub="v-card"><slot /></div>`,
  },
  "v-card-text": {
    template: `<div data-stub="v-card-text"><slot /></div>`,
  },
  "v-card-title": {
    template: `<div data-stub="v-card-title"><slot /></div>`,
  },
  "v-card-actions": {
    template: `<div data-stub="v-card-actions"><slot /></div>`,
  },
  "v-spacer": {
    template: `<div data-stub="v-spacer"></div>`,
  },
  "v-divider": {
    template: `<div data-stub="v-divider"><slot /></div>`,
  },
  "v-row": {
    template: `<div data-stub="v-row"><slot /></div>`,
  },
  "v-col": {
    template: `<div data-stub="v-col"><slot /></div>`,
  },
  "v-chip": {
    template: `<span data-stub="v-chip"><slot /></span>`,
  },
  "v-dialog": {
    props: ["modelValue"],
    emits: ["update:modelValue"],
    template: `<div v-if="modelValue" data-stub="v-dialog"><slot /></div>`,
  },
  "v-btn": {
    props: ["disabled"],
    template: `<button data-stub="v-btn" :disabled="disabled"><slot /></button>`,
  },
  "v-icon": {
    template: `<i data-stub="v-icon"><slot /></i>`,
  },
};

const repository = {
  id: "repository-123",
  name: "helm-charts",
  repository_type: "helm",
  storage_name: "s3-store",
  storage_id: "s3-store",
  storage_usage_bytes: 1024,
  storage_usage_updated_at: "2025-11-10T15:00:00Z",
  active: true,
  auth_enabled: true,
};

describe("BasicRepositoryInfo", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAlerts.success.mockReset();
    mockAlerts.error.mockReset();
  });

  it("renders repository metadata in a themed card layout", () => {
    const wrapper = mount(BasicRepositoryInfo, {
      props: { repository },
      global: {
        stubs: vuetifyStubs,
      },
    });

    expect(wrapper.find('[data-testid="repository-info-card"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="repository-status-chip"]').text()).toContain("Active");
    expect(wrapper.findAll('[data-testid="repository-meta-item"]').length).toBeGreaterThan(0);
  });

  it("renders delete action and a disabled repository toggle placeholder", () => {
    const wrapper = mount(BasicRepositoryInfo, {
      props: { repository },
      global: {
        stubs: vuetifyStubs,
      },
    });

    expect(wrapper.find('[data-testid="repository-toggle"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="repository-toggle"]').attributes("disabled")).toBeDefined();
    expect(wrapper.find('[data-testid="repository-delete"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("Repository activation controls are coming soon.");
  });

  it("opens a confirmation dialog before deleting a repository", async () => {
    const wrapper = mount(BasicRepositoryInfo, {
      props: { repository },
      global: {
        stubs: vuetifyStubs,
      },
    });

    expect(wrapper.find('[data-testid="repository-delete-dialog"]').exists()).toBe(false);

    await wrapper.find('[data-testid="repository-delete"]').trigger("click");

    expect(wrapper.find('[data-testid="repository-delete-dialog"]').exists()).toBe(true);
    expect(mockAlerts.error).not.toHaveBeenCalled();
    expect(mockAlerts.success).not.toHaveBeenCalled();
  });

  it("deletes the repository only after confirmation", async () => {
    const http = await import("@/http");
    (http.default.delete as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce({});

    const wrapper = mount(BasicRepositoryInfo, {
      props: { repository },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await wrapper.find('[data-testid="repository-delete"]').trigger("click");
    await wrapper.find('[data-testid="repository-delete-confirm"]').trigger("click");
    await flushPromises();

    expect(http.default.delete).toHaveBeenCalledWith(`/api/repository/${repository.id}`);
    expect(mockAlerts.success).toHaveBeenCalledWith("Repository deleted", "Repository has been deleted.");
    const router = await import("@/router");
    expect(router.default.push).toHaveBeenCalledWith({ name: "RepositoriesList" });
  });

  it("does not delete when deletion is cancelled", async () => {
    const http = await import("@/http");
    const wrapper = mount(BasicRepositoryInfo, {
      props: { repository },
      global: {
        stubs: vuetifyStubs,
      },
    });

    await wrapper.find('[data-testid="repository-delete"]').trigger("click");
    await wrapper.find('[data-testid="repository-delete-cancel"]').trigger("click");
    await flushPromises();

    expect(http.default.delete).not.toHaveBeenCalled();
    expect(mockAlerts.success).not.toHaveBeenCalled();
  });
});
