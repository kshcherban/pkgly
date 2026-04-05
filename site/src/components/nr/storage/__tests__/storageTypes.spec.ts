import { describe, expect, it, vi } from "vitest";

vi.mock("@/components/nr/storage/local/LocalStorageConfig.vue", () => ({ default: {} }));
vi.mock("@/components/nr/storage/local/UpdateLocalStorageConfig.vue", () => ({ default: {} }));
vi.mock("@/components/nr/storage/s3/S3StorageConfig.vue", () => ({ default: {} }));
vi.mock("@/components/nr/storage/s3/UpdateS3StorageConfig.vue", () => ({ default: {} }));

import { getStorageType, storageTypes } from "@/components/nr/storage/storageTypes";

describe("storageTypes registry", () => {
  it("contains the S3 storage type with the correct discriminators", () => {
    const s3 = getStorageType("s3");
    expect(s3).toBeDefined();
    expect(s3?.configType).toBe("S3");
    const defaults = s3?.defaultSettings();
    expect(defaults).toMatchObject({
      bucket_name: "",
      credentials: {
        access_key: "",
        secret_key: "",
      },
      path_style: true,
    });
  });

  it("keeps Local storage as the default option", () => {
    expect(storageTypes[0]?.value).toBe("Local");
    expect(storageTypes[0]?.configType).toBe("Local");
  });
});
