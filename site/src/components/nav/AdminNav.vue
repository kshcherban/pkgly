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
    </div>
  </SideNav>
</template>

<script setup lang="ts">
import { type PropType } from "vue";
import type { UserResponseType } from "@/types/base";
import SideNav from "./sideNav/SideNav.vue";
import SideNavElement from "./sideNav/SideNavElement.vue";
defineProps({
  user: Object as PropType<UserResponseType>,
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
</style>
