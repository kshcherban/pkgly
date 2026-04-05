import { DockerIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export const DockerFrontendDefinition: FrontendRepositoryType = {
  name: "docker",
  properName: "Docker",
  icons: [
    {
      name: "Docker",
      component: DockerIcon,
      url: "https://www.docker.com/",
      props: {
        color: "#2496ED",
        size: "28",
      },
    },
  ],
};

export type DockerConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: DockerProxyConfig;
    };

export interface DockerProxyConfig {
  upstream_url: string;
  upstream_auth?: {
    username: string;
    password: string;
  };
}

export function defaultDockerProxyConfig(): DockerProxyConfig {
  return {
    upstream_url: "https://registry-1.docker.io",
  };
}
