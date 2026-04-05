export interface GoProxyRoute {
  url: string;
  name?: string;
  priority?: number;
}

export interface GoProxyConfigType {
  routes: GoProxyRoute[];
  go_module_cache_ttl?: number;
}

export type GoConfigType =
  | { type: "Hosted" }
  | { type: "Proxy"; config: GoProxyConfigType };

export function defaultProxy(): GoProxyConfigType {
  return {
    routes: [
      {
        url: "https://proxy.golang.org",
        name: "Go Official Proxy",
        priority: 0,
      },
    ],
    go_module_cache_ttl: 3600,
  };
}

export function getDefaultConfig(): GoConfigType {
  return { type: "Hosted" };
}

export function validateProxyRoute(route: GoProxyRoute): string[] {
  const errors: string[] = [];

  if (!route.url || route.url.trim() === '') {
    errors.push("URL is required");
  }

  try {
    new URL(route.url);
  } catch {
    errors.push("Invalid URL format");
  }

  if (route.priority !== undefined && (route.priority < 0 || route.priority > 100)) {
    errors.push("Priority must be between 0 and 100");
  }

  return errors;
}

export function validateGoConfig(config: GoConfigType): string[] {
  const errors: string[] = [];

  if (config.type === "Proxy") {
    if (!config.config || !config.config.routes || config.config.routes.length === 0) {
      errors.push("At least one proxy route is required");
    }

    if (config.config.routes) {
      config.config.routes.forEach((route, index) => {
        const routeErrors = validateProxyRoute(route);
        routeErrors.forEach(error => {
          errors.push(`Route ${index + 1}: ${error}`);
        });
      });
    }

    if (config.config.go_module_cache_ttl !== undefined) {
      if (config.config.go_module_cache_ttl < 0) {
        errors.push("Cache TTL must be a positive number");
      }
    }
  }

  return errors;
}

// Project helper component for Go repositories
import GoProjectHelper from "./GoProjectHelper.vue";
import { GoIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export const GoFrontendDefinition: FrontendRepositoryType = {
  name: "go",
  properName: "Go",
  projectComponent: {
    component: GoProjectHelper,
    props: {},
  },
  icons: [
    {
      name: "Go",
      component: GoIcon,
      url: "https://golang.org/",
      props: {},
    },
  ],
};