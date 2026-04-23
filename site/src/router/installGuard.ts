import type { Pinia } from "pinia";
import type { RouteLocationNormalized } from "vue-router";
import { sessionStore } from "@/stores/session";
import { siteStore } from "@/stores/site";

export async function installAwareAuthGuard(to: RouteLocationNormalized, pinia: Pinia) {
  const requiresIdentity = to.meta.requiresAuth === true || to.meta.requiresIdentity === true;
  if (!requiresIdentity) {
    return true;
  }

  const site = siteStore(pinia);
  const info = site.siteInfo ?? (await site.getInfo());
  if (info?.is_installed === false) {
    return {
      name: "AdminInstall",
    };
  }

  const store = sessionStore(pinia);
  if (store.session === undefined) {
    await store.updateUser();
  }
  if (store.session === undefined) {
    return {
      name: "login",
      query: { redirect: to.fullPath },
    };
  }
  return true;
}
