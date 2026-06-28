<!-- ABOUTME: Renders admin section navigation and instance version metadata. -->
<!-- ABOUTME: Keeps admin users oriented across management and system pages. -->
<template>
  <SideNav>
    <SideNavElement
      to="/admin/users"
      routeName="UsersList"
      activeTag="admin-users">
      <font-awesome-icon icon="fa-solid fa-users" />
      <span>Users</span>
    </SideNavElement>

    <SideNavElement
      to="/admin/storages"
      routeName="StorageList"
      activeTag="admin-storages">
      <font-awesome-icon icon="fa-solid fa-box-open" />
      <span>Storages</span>
    </SideNavElement>

    <SideNavElement
      to="/admin/repositories"
      routeName="RepositoriesList"
      activeTag="admin-repositories">
      <font-awesome-icon icon="fa-solid fa-boxes-packing" />
      <span>Repositories</span>
    </SideNavElement>

    <div class="navGroup">
      <div class="navGroup__label">
        <font-awesome-icon icon="fa-solid fa-gear" />
        <span>System</span>
      </div>
      <SideNavElement
        to="/admin/system/sso"
        routeName="SystemSingleSignOn">
        <span>Single Sign On</span>
      </SideNavElement>
      <SideNavElement
        to="/admin/system/webhooks"
        routeName="SystemWebhooks">
        <span>Webhooks</span>
      </SideNavElement>
      <SideNavElement
        to="/admin/system/password-rules"
        routeName="SystemPasswordRules">
        <span>Password Rules</span>
      </SideNavElement>
    </div>

    <div
      v-if="versionLabel"
      class="adminNav__version">
      {{ versionLabel }}
    </div>
  </SideNav>
</template>

<script setup lang="ts">
import { computed, type PropType } from "vue";
import type { UserResponseType } from "@/types/base";
import { siteStore } from "@/stores/site";
import SideNav from "./sideNav/SideNav.vue";
import SideNavElement from "./sideNav/SideNavElement.vue";
defineProps({
  user: Object as PropType<UserResponseType>,
});

const site = siteStore();
const versionLabel = computed(() => {
  const version = site.siteInfo?.version;
  if (!version) {
    return "";
  }
  const commitId = site.siteInfo?.commit_id;
  if (commitId) {
    return `Pkgly v${version} (${commitId})`;
  }
  return `Pkgly v${version}`;
});
</script>

<style scoped lang="scss">
.navGroup {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.navGroup__label {
  color: var(--nr-text-primary);
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-weight: 600;
  padding: 0.5rem;
}

.navGroup :deep(.navLink) {
  margin-left: 1.5rem;
}

.adminNav__version {
  margin-top: auto;
  padding: 0.75rem 0.5rem;
  color: var(--nr-text-secondary);
  font-size: 0.75rem;
  line-height: 1.2;
  word-break: break-word;
}
</style>
