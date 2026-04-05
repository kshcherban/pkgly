<template>
  <nav>
    <div>
      <router-link
        to="/"
        id="logoAndHome"
        class="navLink">
        <img
          src="/logo.svg"
          alt="Pkgly logo" />
        <span>Pkgly</span>
      </router-link>
    </div>

    <UserDropDown
      class="right"
      v-if="user" />
    <div v-if="user"></div>
    <RouterLink
      :to="{ name: 'login' }"
      class="navLink right"
      v-else
      >Login</RouterLink
    >
  </nav>
</template>
<script setup lang="ts">
import UserDropDown from "./UserDropDown.vue";
import type { PropType } from "vue";
import type { UserResponseType } from "@/types/base";

defineProps({
  user: {
    type: Object as PropType<UserResponseType>,
    required: false,
  },
});
</script>

<style lang="scss" scoped>
@use "@/assets/styles/theme.scss" as *;
nav {
  display: flex;
  gap: 1rem;
  background-color: $primary-50;
}
#logoAndHome {
  padding-left: 10rem;
  img {
    width: 2rem;
    height: 2rem;
  }
}
.navLink {
  text-decoration: none;
  color: $text;
  font-weight: bold;
  padding: 0.5rem;
  // Align text vertically
  display: flex;
  align-items: center;
  gap: 0.5rem;
  // Box
  border-radius: 0.5rem;
  &:hover,
  &:focus-visible {
    text-decoration: none;
  }
}
.right {
  margin-left: auto;
  margin-right: 5rem;
}
@media screen and (max-width: 600px) {
  nav {
    flex-direction: row;
  }
  #logoAndHome {
    padding-left: 1rem;
  }
  .right {
    margin-left: 0;
    margin-right: 0;
  }
}
</style>
