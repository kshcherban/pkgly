import { PythonIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";
import PythonProjectHelper from "./PythonProjectHelper.vue";
import PythonRepositoryHelper from "./PythonRepositoryHelper.vue";

export interface PythonProxyRoute {
  url: string;
  name?: string;
}

export interface PythonProxyConfigType {
  routes: PythonProxyRoute[];
}

export interface PythonVirtualMemberConfig {
  repository_id: string;
  repository_name: string;
  priority: number;
  enabled: boolean;
}

export interface PythonVirtualConfigType {
  member_repositories: PythonVirtualMemberConfig[];
  resolution_order: "Priority";
  cache_ttl_seconds?: number;
  publish_to?: string | null;
}

export type PythonConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: PythonProxyConfigType;
    }
  | {
      type: "Virtual";
      config: PythonVirtualConfigType;
    };

export function defaultProxy(): PythonProxyConfigType {
  return {
    routes: [
      {
        url: "https://pypi.org/simple",
        name: "PyPI",
      },
    ],
  };
}

export function defaultVirtual(): PythonVirtualConfigType {
  return {
    member_repositories: [],
    resolution_order: "Priority",
    cache_ttl_seconds: 60,
    publish_to: null,
  };
}

export const PythonFrontendDefinition: FrontendRepositoryType = {
  name: "python",
  properName: "Python",
  projectComponent: {
    component: PythonProjectHelper,
  },
  fullProjectComponent: {
    component: PythonRepositoryHelper,
  },
  icons: [
    {
      name: "Python",
      component: PythonIcon,
      url: "https://www.python.org/",
      props: {},
    },
  ],
};
