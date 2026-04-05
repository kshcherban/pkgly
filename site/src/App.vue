<template>
  <v-app>
    <AppBar :user="user" />
    <v-main>
      <RouterView v-slot="{ Component, route }">
        <div
          class="contentWithSideBar"
          v-if="route.meta.sideBar">
          <component :is="route.meta.sideBar" />
          <v-slide-x-transition mode="out-in">
            <component :is="Component" :key="route.fullPath" />
          </v-slide-x-transition>
        </div>
        <v-slide-x-transition mode="out-in" v-else>
          <component :is="Component" :key="route.fullPath" />
        </v-slide-x-transition>
      </RouterView>
    </v-main>
    <GlobalAlerts />
  </v-app>
</template>
<script setup lang="ts">
import { siteStore } from "./stores/site";
import router from "./router";
import AppBar from "./components/layout/AppBar.vue";
import { sessionStore } from "./stores/session";
import { computed } from "vue";
import { apiURL } from "./config";
import routesJson from "../src/router/routes.json";
import GlobalAlerts from "@/components/core/GlobalAlerts.vue";

const site = siteStore();
const session = sessionStore();
const user = computed(() => session.user);

if (import.meta.env.MODE === "development") {
  const routes: Array<{ path: string; name: string }> = [];

  for (const route of router.options.routes) {
    if (route.meta?.skipRoutesJson === true) {
      continue;
    }
    routes.push({ path: route.path, name: route.name as string });
  }
  for (const route of routes) {
    const foundRoute = routesJson.find((r) => r.path === route.path && (route.name = r.name));
    if (!foundRoute) {
      console.error(`route not found: ${route.path} update routes.json`);
    } else {
      console.log(`route found: ${route.path}`);
    }
  }
  console.log("");
  console.log(JSON.stringify(routes));
}
console.log(`apiURL: ${apiURL}`);
async function init() {
  const info = await site.getInfo();
  if (info == undefined) {
    console.log("info is undefined");
    return;
  }
  console.log(info);

  if (!info?.is_installed) {
    router.push("/admin/install");
  }
  const session = sessionStore();
  const user = await session.updateUser();
  if (user == undefined) {
    console.log("user is undefined");
    return;
  }
}
init();
</script>
<style scoped lang="scss">
.contentWithSideBar {
  display: flex;
  height: 90vh;
  main {
    flex: 1;
    padding: 1rem;
  }
}
</style>
