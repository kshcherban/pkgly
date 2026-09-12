// ABOUTME: Exercises storage deletion through the real HTTP client and Pinia store.
// ABOUTME: Uses a loopback HTTP server to validate cascade confirmation and retries.
import { flushPromises, mount } from "@vue/test-utils";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { createPinia, setActivePinia, type Pinia } from "pinia";

const routerMock = vi.hoisted(() => ({
  currentRoute: {
    value: {
      params: { id: "storage-123" },
    },
  },
  push: vi.fn(),
}));

vi.mock("@/router", () => ({
  default: routerMock,
}));

const mockAlerts = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

vi.mock("@/components/nr/storage/storageTypes", () => ({
  storageTypes: [
    {
      value: "Test",
      updateComponent: { template: `<div data-testid="storage-config" />` },
    },
  ],
}));

import http from "@/http";
import { useRepositoryStore } from "@/stores/repositories";
import type { RepositoryWithStorageName } from "@/types/repository";
import type { StorageItem } from "@/components/nr/storage/storageTypes";
import ViewStorage from "../ViewStorage.vue";

interface DeleteResponse {
  status: number;
  body?: unknown;
}

const storage = {
  id: "storage-123",
  name: "primary",
  storage_type: "Test",
  config: { type: "Test", settings: {} },
  active: true,
  created_at: "2026-01-01T00:00:00Z",
};

const serverState = {
  storage,
  deleteResponses: [] as DeleteResponse[],
  deleteRequests: [] as string[],
};

let server: Server;
let pinia: Pinia;

function handleRequest(req: IncomingMessage, res: ServerResponse) {
  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  if (req.method === "GET" && url.pathname === "/api/storage/storage-123") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(serverState.storage));
    return;
  }
  if (req.method === "DELETE" && url.pathname === "/api/storage/storage-123") {
    serverState.deleteRequests.push(`${url.pathname}${url.search}`);
    const next = serverState.deleteResponses.shift() ?? { status: 204 };
    res.writeHead(next.status, { "Content-Type": "application/json" });
    res.end(next.body ? JSON.stringify(next.body) : "");
    return;
  }
  res.writeHead(404);
  res.end();
}

beforeAll(async () => {
  server = createServer(handleRequest);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
  const { port } = server.address() as AddressInfo;
  http.defaults.baseURL = `http://127.0.0.1:${port}`;
  http.defaults.adapter = "http";
});

afterAll(async () => {
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

const vuetifyStubs = {
  TextInput: { template: `<div><slot /></div>` },
  TwoByFormBox: { template: `<div><slot /></div>` },
  "v-btn": {
    props: ["disabled"],
    template: `<button data-stub="v-btn" :disabled="disabled"><slot /></button>`,
  },
  "v-icon": { template: `<i data-stub="v-icon"><slot /></i>` },
  "v-card": { template: `<div data-stub="v-card"><slot /></div>` },
  "v-card-title": { template: `<div data-stub="v-card-title"><slot /></div>` },
  "v-card-text": { template: `<div data-stub="v-card-text"><slot /></div>` },
  "v-card-actions": { template: `<div data-stub="v-card-actions"><slot /></div>` },
  "v-spacer": { template: `<div data-stub="v-spacer"></div>` },
  "v-dialog": {
    props: ["modelValue"],
    emits: ["update:modelValue"],
    template: `<div v-if="modelValue" data-stub="v-dialog"><slot /></div>`,
  },
};

function repositoryItem(id: string, storageId: string): RepositoryWithStorageName {
  return {
    id,
    storage_name: `storage-${storageId}`,
    storage_id: storageId,
    name: id,
    repository_type: "maven",
    active: true,
    visibility: "Public",
    updated_at: "2026-01-01T00:00:00Z",
    created_at: "2026-01-01T00:00:00Z",
    auth_enabled: false,
    storage_usage_bytes: null,
    storage_usage_updated_at: null,
  } as unknown as RepositoryWithStorageName;
}

function storageItem(): StorageItem {
  return {
    id: "storage-123",
    name: "primary",
    storage_type: "Test",
    config: { type: "Test", settings: {} },
    active: true,
    created_at: "2026-01-01T00:00:00Z" as unknown as Date,
  } as unknown as StorageItem;
}

function seedStore(): ReturnType<typeof useRepositoryStore> {
  const store = useRepositoryStore();
  store.storages = [storageItem()];
  store.repositories = {
    "repo-1": repositoryItem("repo-1", "storage-123"),
    "repo-2": repositoryItem("repo-2", "other-storage"),
  };
  return store;
}

async function mountView() {
  const wrapper = mount(ViewStorage, {
    global: { plugins: [pinia], stubs: vuetifyStubs },
  });
  await vi.waitFor(() => {
    expect(wrapper.find('[data-testid="storage-delete"]').exists()).toBe(true);
  });
  return wrapper;
}

describe("ViewStorage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAlerts.success.mockReset();
    mockAlerts.error.mockReset();
    routerMock.push.mockReset();
    serverState.storage = storage;
    serverState.deleteResponses = [];
    serverState.deleteRequests = [];
    pinia = createPinia();
    setActivePinia(pinia);
  });

  it("deletes an empty storage immediately and evicts its cached repositories", async () => {
    serverState.deleteResponses = [{ status: 204 }];
    const store = seedStore();
    const wrapper = await mountView();

    await wrapper.find('[data-testid="storage-delete"]').trigger("click");
    await vi.waitFor(() => {
      expect(serverState.deleteRequests).toHaveLength(1);
    });

    expect(serverState.deleteRequests).toEqual(["/api/storage/storage-123?cascade=false"]);
    expect(wrapper.find('[data-testid="storage-delete-dialog"]').exists()).toBe(false);
    expect(store.storages.map((item) => item.id)).not.toContain("storage-123");
    expect(Object.keys(store.repositories)).toEqual(["repo-2"]);
    expect(mockAlerts.success).toHaveBeenCalledWith("Storage deleted", "The storage has been deleted.");
    expect(routerMock.push).toHaveBeenCalledWith({ name: "StorageList" });
  });

  it("asks for confirmation when the storage contains repositories and retries with cascade", async () => {
    serverState.deleteResponses = [
      {
        status: 409,
        body: {
          message: "Storage contains repositories.",
          details: { code: "storage_not_empty", repository_count: 2 },
        },
      },
      { status: 204 },
    ];
    seedStore();
    const wrapper = await mountView();

    await wrapper.find('[data-testid="storage-delete"]').trigger("click");
    await vi.waitFor(() => {
      expect(wrapper.find('[data-testid="storage-delete-dialog"]').exists()).toBe(true);
    });

    const dialog = wrapper.find('[data-testid="storage-delete-dialog"]');
    expect(dialog.text()).toContain("2");
    expect(mockAlerts.error).not.toHaveBeenCalled();

    await wrapper.find('[data-testid="storage-delete-confirm"]').trigger("click");
    await vi.waitFor(() => {
      expect(serverState.deleteRequests).toHaveLength(2);
    });

    expect(serverState.deleteRequests).toEqual([
      "/api/storage/storage-123?cascade=false",
      "/api/storage/storage-123?cascade=true",
    ]);
    expect(mockAlerts.success).toHaveBeenCalledWith("Storage deleted", "The storage has been deleted.");
    expect(routerMock.push).toHaveBeenCalledWith({ name: "StorageList" });
  });

  it("does not send a second request when confirmation is cancelled", async () => {
    serverState.deleteResponses = [
      {
        status: 409,
        body: {
          message: "Storage contains repositories.",
          details: { code: "storage_not_empty", repository_count: 3 },
        },
      },
    ];
    seedStore();
    const wrapper = await mountView();

    await wrapper.find('[data-testid="storage-delete"]').trigger("click");
    await vi.waitFor(() => {
      expect(wrapper.find('[data-testid="storage-delete-dialog"]').exists()).toBe(true);
    });
    await wrapper.find('[data-testid="storage-delete-cancel"]').trigger("click");
    await flushPromises();

    expect(serverState.deleteRequests).toHaveLength(1);
    expect(wrapper.find('[data-testid="storage-delete-dialog"]').exists()).toBe(false);
    expect(mockAlerts.success).not.toHaveBeenCalled();
    expect(routerMock.push).not.toHaveBeenCalled();
  });

  it("surfaces actionable guidance when cleanup fails partway", async () => {
    const message =
      "Failed to remove repository contents. The storage and its repositories were kept so deletion can be retried; files already removed cannot be restored.";
    serverState.deleteResponses = [
      {
        status: 500,
        body: {
          message,
          details: {
            code: "storage_cleanup_failed",
            repository_id: "repo-1",
            repositories_remaining: 2,
            detail: "Permission denied",
          },
        },
      },
    ];
    seedStore();
    const wrapper = await mountView();

    await wrapper.find('[data-testid="storage-delete"]').trigger("click");
    await vi.waitFor(() => {
      expect(mockAlerts.error).toHaveBeenCalled();
    });

    expect(mockAlerts.error).toHaveBeenCalledWith(
      "Storage deletion failed",
      expect.stringContaining("retried"),
    );
    expect(wrapper.find('[data-testid="storage-delete-dialog"]').exists()).toBe(false);
  });

  it("shows a generic error for unexpected failures", async () => {
    serverState.deleteResponses = [{ status: 500, body: {} }];
    seedStore();
    const wrapper = await mountView();

    await wrapper.find('[data-testid="storage-delete"]').trigger("click");
    await vi.waitFor(() => {
      expect(mockAlerts.error).toHaveBeenCalled();
    });

    expect(mockAlerts.error).toHaveBeenCalledWith(
      "Failed to delete storage",
      "An error occurred while deleting the storage.",
    );
    expect(routerMock.push).not.toHaveBeenCalled();
  });
});
