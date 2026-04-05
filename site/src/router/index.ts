import { createRouter, createWebHistory } from "vue-router";
import HomeView from "../views/HomeView.vue";
import BrowseView from "@/views/BrowseView.vue";
import LoginView from "@/views/LoginView.vue";
import LogoutView from "@/views/LogoutView.vue";
import OAuthDeniedView from "@/views/OAuthDeniedView.vue";
import type { Component } from "vue";

import { adminRoutes } from "@/views/admin/adminRoutes";
import { profileRoutes } from "@/views/profile/profileRoutes";
import { projectRoutes } from "@/views/projects";
import NotFound from "@/views/NotFound.vue";
import { repositoryPages } from "@/views/repositoryPages";
declare module "vue-router" {
  interface RouteMeta {
    requiresAuth?: boolean;
    requiresRepositoryManager?: boolean;
    requiresUserManager?: boolean;
    sideBar?: Component;
    tag?: string;
    skipRoutesJson?: boolean;
  }
}
const routes = [
  {
    path: "/",
    name: "home",
    component: HomeView,
    meta: {
      skipRoutesJson: true,
      requiresAuth: true,
    },
  },

  {
    path: "/browse/:id/:catchAll(.*)?",
    name: "Browse",
    component: BrowseView,
  },

  {
    path: "/login",
    name: "login",
    component: LoginView,
  },
  {
    path: "/logout",
    name: "logout",
    component: LogoutView,
  },
  {
    path: "/oauth/denied",
    name: "oauth-denied",
    component: OAuthDeniedView,
    meta: {
      skipRoutesJson: true,
    },
  },
  ...repositoryPages,
  ...adminRoutes,
  ...profileRoutes,
  ...projectRoutes,
  {
    path: "/:pathMatch(.*)*",
    name: "not-found",
    component: NotFound,
    meta: {
      skipRoutesJson: true,
    },
  },
];
const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: routes,
});

export default router;
