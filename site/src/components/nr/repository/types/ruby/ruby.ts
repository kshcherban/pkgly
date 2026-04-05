import { RubyGemsIcon, RubyIcon } from "vue3-simple-icons";
import type { FrontendRepositoryType } from "@/types/repository";

export interface RubyProxyConfigType {
  upstream_url: string;
  revalidation_ttl_seconds?: number;
}

export type RubyConfigType =
  | {
      type: "Hosted";
    }
  | {
      type: "Proxy";
      config: RubyProxyConfigType;
    };

export function defaultProxy(): RubyProxyConfigType {
  return {
    upstream_url: "https://rubygems.org",
  };
}

export const RubyFrontendDefinition: FrontendRepositoryType = {
  name: "ruby",
  properName: "RubyGems",
  icons: [
    {
      name: "RubyGems",
      component: RubyGemsIcon,
      url: "https://rubygems.org/",
      props: {},
    },
    {
      name: "Ruby",
      component: RubyIcon,
      url: "https://www.ruby-lang.org/",
      props: {},
    },
  ],
};

