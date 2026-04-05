import type { FrontendRepositoryType } from "@/types/repository";
import CargoProjectHelper from "./CargoProjectHelper.vue";
import CargoIcon from "./CargoIcon.vue";

export type CargoConfigType = {
  type: "Hosted";
};

export function getDefaultConfig(): CargoConfigType {
  return { type: "Hosted" };
}

export const CargoFrontendDefinition: FrontendRepositoryType = {
  name: "cargo",
  properName: "Cargo",
  projectComponent: {
    component: CargoProjectHelper,
  },
  icons: [
    {
      name: "Rust",
      component: CargoIcon,
      url: "https://www.rust-lang.org/",
      props: {},
    },
  ],
};
