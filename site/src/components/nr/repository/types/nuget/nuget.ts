import { DotNetIcon, NuGetIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export interface NugetProxyConfigType {
  upstream_url: string;
}

export interface NugetVirtualMemberConfig {
  repository_id: string;
  repository_name: string;
  priority: number;
  enabled: boolean;
}

export interface NugetVirtualConfigType {
  member_repositories: NugetVirtualMemberConfig[];
  resolution_order: "Priority";
  cache_ttl_seconds?: number;
  publish_to?: string | null;
}

export type NugetConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: NugetProxyConfigType;
    }
  | {
      type: "Virtual";
      config: NugetVirtualConfigType;
    };

export function defaultProxy(): NugetProxyConfigType {
  return {
    upstream_url: "https://api.nuget.org/v3/index.json",
  };
}

export function defaultVirtual(): NugetVirtualConfigType {
  return {
    member_repositories: [],
    resolution_order: "Priority",
    cache_ttl_seconds: 60,
    publish_to: null,
  };
}

export const NugetFrontendDefinition: FrontendRepositoryType = {
  name: "nuget",
  properName: "NuGet",
  icons: [
    {
      name: "NuGet",
      component: NuGetIcon,
      url: "https://www.nuget.org/",
      props: {},
    },
    {
      name: ".NET",
      component: DotNetIcon,
      url: "https://dotnet.microsoft.com/",
      props: {},
    },
  ],
};
