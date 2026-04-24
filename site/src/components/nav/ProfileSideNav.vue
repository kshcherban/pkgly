<template>
  <SideNav>
    <SideNavElement
      to="/profile"
      routeName="profile">
      <font-awesome-icon icon="fa-solid fa-gear" />
      <span>Profile</span>
    </SideNavElement>
    <SideNavElement
      to="/profile/login"
      routeName="profileLogin">
      <font-awesome-icon icon="fa-solid fa-gear" />
      <span>Login</span>
    </SideNavElement>
    <SideNavElement
      to="/profile/tokens"
      routeName="profileTokens"
      :activeTag="profileTokenTag">
      <font-awesome-icon icon="fa-solid fa-gear" />
      <span>Tokens</span>
    </SideNavElement>
  </SideNav>
</template>
<script setup lang="ts">
import { profileTokenTag } from "@/views/profile/profileRoutes";
import SideNav from "./sideNav/SideNav.vue";
import SideNavElement from "./sideNav/SideNavElement.vue";
</script>

<style scoped lang="scss">
// ProfileSideNav uses the new SideNav component which already handles proper theming
</style>
