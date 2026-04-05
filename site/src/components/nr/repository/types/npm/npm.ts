import { NpmIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";
import NPMProjectHelper from "./NPMProjectHelper.vue";

export interface NpmProxyRoute {
  url: string;
  name?: string;
}

export interface NpmProxyConfigType {
  routes: NpmProxyRoute[];
}

export interface NpmVirtualMemberConfig {
  repository_id: string;
  repository_name: string;
  priority: number;
  enabled: boolean;
}

export interface NpmVirtualConfigType {
  member_repositories: NpmVirtualMemberConfig[];
  resolution_order: "Priority";
  cache_ttl_seconds?: number;
  publish_to?: string | null;
}

export type NPMConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: NpmProxyConfigType;
    }
  | {
      type: "Virtual";
      config: NpmVirtualConfigType;
    };

export function defaultProxy(): NpmProxyConfigType {
  return {
    routes: [
      {
        url: "https://registry.npmjs.org",
        name: "npmjs",
      },
    ],
  };
}

export function defaultVirtual(): NpmVirtualConfigType {
  return {
    member_repositories: [],
    resolution_order: "Priority",
    cache_ttl_seconds: 60,
    publish_to: null,
  };
}

export const NpmFrontendDefinition: FrontendRepositoryType = {
  name: "npm",
  properName: "NPM",
  projectComponent: {
    component: NPMProjectHelper,
    props: {},
  },
  icons: [
    {
      name: "NPM",
      component: NpmIcon,
      url: "https://www.npmjs.com/",
      props: {},
    },
  ],
};
