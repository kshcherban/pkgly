import { HelmIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export type HelmRepositoryMode = "http" | "oci";

export interface HelmRepositoryConfig {
  overwrite: boolean;
  index_cache_ttl?: number;
  mode: HelmRepositoryMode;
  public_base_url?: string;
  max_chart_size?: number;
  max_file_count?: number;
}

export function defaultHelmConfig(): HelmRepositoryConfig {
  return {
    overwrite: false,
    index_cache_ttl: 300,
    mode: "http",
    public_base_url: undefined,
    max_chart_size: 10 * 1024 * 1024,
    max_file_count: 1024,
  };
}

export const helmModeOptions = [
  { value: "http", label: "HTTP chart repository" },
  { value: "oci", label: "OCI registry only" },
];

export const HelmFrontendDefinition: FrontendRepositoryType = {
  name: "helm",
  properName: "Helm",
  icons: [
    {
      name: "Helm",
      component: HelmIcon,
      url: "https://helm.sh/",
      props: {
        color: "#0F1689",
        size: "28",
      },
    },
  ],
};
