// ABOUTME: Unit tests for the repository store cache eviction on storage deletion.
// ABOUTME: Ensures repositories belonging to a deleted storage are removed from cache.
import { beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useRepositoryStore } from "../repositories";
import type { StorageItem } from "@/components/nr/storage/storageTypes";
import type { RepositoryWithStorageName } from "@/types/repository";

function storage(id: string): StorageItem {
  return {
    id,
    name: `storage-${id}`,
    storage_type: "Local",
    config: { type: "Local", settings: { path: "/tmp" } },
    active: true,
    created_at: "2026-01-01T00:00:00Z" as unknown as Date,
  } as unknown as StorageItem;
}

function repository(id: string, storageId: string): RepositoryWithStorageName {
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

describe("useRepositoryStore.removeStorage", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("evicts the storage and every repository that belongs to it", () => {
    const store = useRepositoryStore();
    store.storages = [storage("s1"), storage("s2")];
    store.repositories = {
      r1: repository("r1", "s1"),
      r2: repository("r2", "s2"),
      r3: repository("r3", "s1"),
    };

    store.removeStorage("s1");

    expect(store.storages.map((item) => item.id)).toEqual(["s2"]);
    expect(Object.keys(store.repositories)).toEqual(["r2"]);
  });
});
