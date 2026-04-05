import http from "@/http";
import MavenConfig from "@/components/nr/repository/types/maven/MavenConfig.vue";
import PythonConfig from "@/components/nr/repository/types/python/PythonConfig.vue";
import PhpConfig from "@/components/nr/repository/types/php/PhpConfig.vue";
import GoConfig from "@/components/nr/repository/types/go/GoConfig.vue";
import HelmConfig from "@/components/nr/repository/types/helm/HelmConfig.vue";
import CargoConfig from "@/components/nr/repository/types/cargo/CargoConfig.vue";
import DebConfig from "@/components/nr/repository/types/deb/DebConfig.vue";
import DockerConfig from "@/components/nr/repository/types/docker/DockerConfig.vue";
import RubyConfig from "@/components/nr/repository/types/ruby/RubyConfig.vue";

import type { Component } from "vue";
import RepositoryAuthConfig from "@/components/admin/repository/configs/RepositoryAuthConfig.vue";

import { MavenFrontendDefinition } from "@/components/nr/repository/types/maven/maven";
import { PythonFrontendDefinition } from "@/components/nr/repository/types/python/python";
import { PhpFrontendDefinition } from "@/components/nr/repository/types/php/php";
import { NpmFrontendDefinition } from "@/components/nr/repository/types/npm/npm";
import { RubyFrontendDefinition } from "@/components/nr/repository/types/ruby/ruby";
import { GoFrontendDefinition } from "@/components/nr/repository/types/go/go";
import { HelmFrontendDefinition } from "@/components/nr/repository/types/helm/helm";
import { DockerFrontendDefinition } from "@/components/nr/repository/types/docker/docker";
import { CargoFrontendDefinition } from "@/components/nr/repository/types/cargo/cargo";
import { DebFrontendDefinition } from "@/components/nr/repository/types/deb/deb";
import NPMConfig from "@/components/nr/repository/types/npm/NPMConfig.vue";
import type { RepositoryActionsType } from "./user";
export { createRepositoryRoute } from "./repositoryRoute";

export interface RepositoryTypeDescription {
  type_name: string;
  name: string;
  description: string;
  documentation_url?: string;
  is_stable: boolean;
  required_configs: string[];
}
enum Visibility {
  Private = "Private",
  Public = "Public",
  Hidden = "Hidden",
}
export interface RepositoryWithStorageName {
  id: string;
  storage_name: string;
  storage_id: string;
  name: string;
  repository_type: string;
  repository_kind?: string | null;
  active: boolean;
  visibility: Visibility;
  updated_at: string;
  created_at: string;
  auth_enabled: boolean;
  storage_usage_bytes: number | null;
  storage_usage_updated_at: string | null;
}
export interface ConfigDescription {
  name: string;
  description: string;
  documentation_url?: string;
}
export interface ConfigType {
  name: string;
  title: string;
  component: Component;
}

export const configTypes: ConfigType[] = [
  {
    name: "maven",
    title: "Maven Repository Config",
    component: MavenConfig,
  },
  {
    name: "cargo",
    title: "Cargo Repository Config",
    component: CargoConfig,
  },
  {
    name: "python",
    title: "Python Repository Config",
    component: PythonConfig,
  },
  {
    name: "auth",
    title: "Repository Authentication",
    component: RepositoryAuthConfig,
  },
  {
    name: "npm",
    title: "NPM Repository Config",
    component: NPMConfig,
  },
  {
    name: "ruby",
    title: "Ruby Repository Config",
    component: RubyConfig,
  },
  {
    name: "php",
    title: "PHP Repository Config",
    component: PhpConfig,
  },
  {
    name: "go",
    title: "Go Repository Config",
    component: GoConfig,
  },
  {
    name: "helm",
    title: "Helm Repository Config",
    component: HelmConfig,
  },
  {
    name: "docker",
    title: "Docker Repository Config",
    component: DockerConfig,
  },
  {
    name: "deb",
    title: "Debian Repository Config",
    component: DebConfig,
  },
];
export interface RepositoryIconDef {
  name: string;
  component: Component;
  url: string;
  props: Record<string, any>;
}
export interface FrontendRepositoryType {
  name: string;
  properName: string;
  projectComponent?: {
    component: Component;
    props?: Record<string, any>;
  };
  fullProjectComponent?: {
    component: Component;
    props?: Record<string, any>;
  };
  icons: Array<RepositoryIconDef>;
}
export const repositoryTypes: FrontendRepositoryType[] = [
  MavenFrontendDefinition,
  CargoFrontendDefinition,
  NpmFrontendDefinition,
  RubyFrontendDefinition,
  PythonFrontendDefinition,
  PhpFrontendDefinition,
  GoFrontendDefinition,
  HelmFrontendDefinition,
  DockerFrontendDefinition,
  DebFrontendDefinition,
];

const packageViewRepositoryTypeSet = new Set(
  repositoryTypes.map((repositoryType) => repositoryType.name.toLowerCase()),
);

export function supportsRepositoryPackageView(repositoryType: string | null | undefined): boolean {
  if (typeof repositoryType !== "string") {
    return false;
  }
  return packageViewRepositoryTypeSet.has(repositoryType.toLowerCase());
}

export function shouldDisplayRepositoryIndexingWarning(
  repositoryType: string | null | undefined,
  repositoryKind: string | null | undefined,
): boolean {
  const normalizedType = typeof repositoryType === "string" ? repositoryType.toLowerCase() : null;
  const normalizedKind = typeof repositoryKind === "string" ? repositoryKind.toLowerCase() : null;
  return !(normalizedType === "ruby" && normalizedKind === "proxy");
}

export function findRepositoryType(name: string): FrontendRepositoryType | undefined {
  return repositoryTypes.find((repositoryType) => repositoryType.name === name);
}

export function getConfigType(name: string): ConfigType | undefined {
  return configTypes.find((configType) => configType.name === name);
}

export async function getConfigTypeDefault(name: string): Promise<any> {
  return await http
    .get<any>(`/api/repository/config/${name}/default`)
    .then((response: any) => {
      return response.data;
    })
    .catch((error: any) => {
      console.error(error);
    });
}

export async function validateConfig(name: string, config: any): Promise<any> {
  return await http
    .post<any>(`/api/repository/config/${name}/validate`, config)
    .then((response: any) => {
      return response.data;
    })
    .catch((error: any) => {
      console.error(error);
    });
}
export interface RepositoryToActions {
  repositoryId: string;
  actions: RepositoryActionsType;
}
