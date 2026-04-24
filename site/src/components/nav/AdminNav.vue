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

    <SideNavElement
      to="/admin/system"
      routeName="SystemSettings">
      <font-awesome-icon icon="fa-solid fa-gear" />
      <span>System</span>
    </SideNavElement>
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
