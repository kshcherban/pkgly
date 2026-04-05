<template>
  <SideNav>
    <ExpandingSideNav :openIfHasTag="adminUserTag">
      <template #button>
        <SideNavElement
          to="/admin/users"
          routeName="UserList">
          <font-awesome-icon icon="fa-solid fa-users" />
          <span>Users</span>
        </SideNavElement>
      </template>
      <template #content>
        <SideNavElement
          to="/admin/user/create"
          routeName="UserCreate">
          <font-awesome-icon icon="fa-solid fa-user-plus" />
          <span>Create User</span>
        </SideNavElement>
      </template>
    </ExpandingSideNav>
    <ExpandingSideNav :openIfHasTag="adminStorageTag">
      <template #button>
        <SideNavElement
          to="/admin/storages"
          routeName="StorageList">
          <font-awesome-icon icon="fa-solid fa-box-open" />
          <span>Storages</span>
        </SideNavElement>
      </template>
      <template #content>
        <SideNavElement
          to="/admin/storage/create"
          routeName="StorageCreate">
          <font-awesome-icon icon="fa-solid fa-box-open" />
          <span>Create Storage</span>
        </SideNavElement>
      </template>
    </ExpandingSideNav>
    <ExpandingSideNav :openIfHasTag="adminRepositoryTag">
      <template #button>
        <SideNavElement
          to="/admin/repositories"
          routeName="RepositoriesList">
          <font-awesome-icon icon="fa-solid fa-boxes-packing" />
          <span>Repositories</span>
        </SideNavElement>
      </template>
      <template #content>
        <SideNavElement
          to="/admin/repositories/create"
          routeName="RepositoryCreate">
          <font-awesome-icon icon="fa-solid fa-boxes-packing" />
          <span>Create Repository</span>
        </SideNavElement>
      </template>
    </ExpandingSideNav>

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
import { adminRepositoryTag, adminStorageTag, adminUserTag } from "@/views/admin/adminRoutes";
import SideNav from "./sideNav/SideNav.vue";
import ExpandingSideNav from "./sideNav/ExpandingSideNav.vue";
import SideNavElement from "./sideNav/SideNavElement.vue";
defineProps({
  user: Object as PropType<UserResponseType>,
});
</script>
