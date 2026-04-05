import LocalStorageConfig from "@/components/nr/storage/local/LocalStorageConfig.vue";
import UpdateLocalStorageConfig from "@/components/nr/storage/local/UpdateLocalStorageConfig.vue";
import S3StorageConfig from "@/components/nr/storage/s3/S3StorageConfig.vue";
import UpdateS3StorageConfig from "@/components/nr/storage/s3/UpdateS3StorageConfig.vue";

type StorageConfigDiscriminator = "Local" | "S3";

export type StorageSettings = LocalConfig | S3StorageSettings;

interface StorageType {
  label: string;
  value: string;
  title: string;
  description: string;
  component: any;
  updateComponent: any;
  configType: StorageConfigDiscriminator;
  defaultSettings: () => StorageSettings;
}

export interface LocalConfig {
  path: string;
}

export interface S3CredentialsConfig {
  access_key?: string;
  secret_key?: string;
  session_token?: string;
  role_arn?: string;
  role_session_name?: string;
  external_id?: string;
}

export interface S3CacheSettings {
  enabled: boolean;
  path?: string;
  max_bytes: number;
  max_entries: number;
}

export interface S3StorageSettings {
  bucket_name: string;
  region?: string;
  custom_region?: string;
  endpoint?: string;
  credentials: S3CredentialsConfig;
  path_style: boolean;
  cache: S3CacheSettings;
}

export type StorageTypeConfig =
  | {
      type: "Local";
      settings: LocalConfig;
    }
  | {
      type: "S3";
      settings: S3StorageSettings;
    };

export const storageTypes: Array<StorageType> = [
  {
    label: "Local",
    value: "Local",
    title: "Local Storage Configuration",
    description: "Local storage configuration allows you to store data on your local machine.",
    component: LocalStorageConfig,
    updateComponent: UpdateLocalStorageConfig,
    configType: "Local",
    defaultSettings: () => ({
      path: "",
    }),
  },
  {
    label: "S3 / Object Storage",
    value: "s3",
    title: "S3 Storage Configuration",
    description: "Back repositories with Amazon S3 or any compatible object storage endpoint.",
    component: S3StorageConfig,
    updateComponent: UpdateS3StorageConfig,
    configType: "S3",
    defaultSettings: () => ({
      bucket_name: "",
      region: undefined,
      custom_region: undefined,
      endpoint: undefined,
      credentials: {
        access_key: "",
        secret_key: "",
        session_token: "",
        role_arn: "",
        role_session_name: "",
        external_id: "",
      },
      path_style: true,
      cache: {
        enabled: false,
        path: "",
        max_bytes: 536870912,
        max_entries: 2048,
      },
    }),
  },
];

export function getStorageType(value: string): StorageType | undefined {
  return storageTypes.find((type) => type.value === value);
}

export interface StorageItem {
  id: string;
  name: string;
  storage_type: string;
  config: StorageTypeConfig;
  active: boolean;
  created_at: Date;
}
