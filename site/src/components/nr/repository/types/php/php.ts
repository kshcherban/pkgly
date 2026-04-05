import { PhpIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";
import PhpProjectHelper from "./PhpProjectHelper.vue";
import PhpRepositoryHelper from "./PhpRepositoryHelper.vue";

export interface PhpProxyRoute {
  url: string;
  name?: string;
}

export interface PhpProxyConfig {
  routes: PhpProxyRoute[];
}

export type PhpConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: PhpProxyConfig;
    };

export const defaultProxy = (): PhpProxyConfig => ({
  routes: [
    {
      url: "https://repo.packagist.org",
      name: "Packagist",
    },
  ],
});

export const PhpFrontendDefinition: FrontendRepositoryType = {
  name: "php",
  properName: "PHP Composer",
  projectComponent: {
    component: PhpProjectHelper,
  },
  fullProjectComponent: {
    component: PhpRepositoryHelper,
  },
  icons: [
    {
      name: "PHP",
      component: PhpIcon,
      url: "https://www.php.net/",
      props: {},
    },
  ],
};
