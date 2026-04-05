import { DebianIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export interface DebHostedConfig {
  distributions: string[];
  components: string[];
  architectures: string[];
}

export interface DebProxyDistsLayoutConfig {
  distributions: string[];
  components: string[];
  architectures: string[];
}

export interface DebProxyFlatLayoutConfig {
  distribution: string;
  architectures: string[];
}

export type DebProxyLayout =
  | { type: "dists"; config: DebProxyDistsLayoutConfig }
  | { type: "flat"; config: DebProxyFlatLayoutConfig };

export type DebProxyRefreshSchedule =
  | { type: "interval_seconds"; config: { interval_seconds: number } }
  | { type: "cron"; config: { expression: string } };

export interface DebProxyRefreshConfig {
  enabled: boolean;
  schedule: DebProxyRefreshSchedule;
}

export interface DebProxyConfig {
  upstream_url: string;
  layout: DebProxyLayout;
  refresh?: DebProxyRefreshConfig;
}

export type DebRepositoryConfig = DebHostedConfig | { type: "proxy"; config: DebProxyConfig };

export function isDebProxyConfig(value: unknown): value is { type: "proxy"; config: DebProxyConfig } {
  if (!value || typeof value !== "object") {
    return false;
  }
  const record = value as Record<string, unknown>;
  return record.type === "proxy" && typeof record.config === "object" && record.config !== null;
}

export function defaultDebHostedConfig(): DebHostedConfig {
  return {
    distributions: ["stable"],
    components: ["main"],
    architectures: ["amd64", "all"],
  };
}

export function defaultDebProxyConfig(): DebProxyConfig {
  return {
    upstream_url: "https://deb.debian.org/debian",
    layout: {
      type: "dists",
      config: defaultDebHostedConfig(),
    },
    refresh: {
      enabled: false,
      schedule: { type: "interval_seconds", config: { interval_seconds: 3600 } },
    },
  };
}

export function defaultDebConfig(): DebRepositoryConfig {
  return defaultDebHostedConfig();
}

export const DebFrontendDefinition: FrontendRepositoryType = {
  name: "deb",
  properName: "Debian",
  icons: [
    {
      name: "Debian",
      component: DebianIcon,
      url: "https://www.debian.org/",
      props: {},
    },
  ],
};
