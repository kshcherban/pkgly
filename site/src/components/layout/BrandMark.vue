<!-- ABOUTME: Shared Pkgly brand lockup — logo avatar + wordmark, linking home. -->
<!-- ABOUTME: Horizontal (app bar) by default, stacked variant for centered heroes. -->
<template>
  <router-link
    to="/"
    class="brand-mark d-flex align-center text-decoration-none"
    :class="{ 'brand-mark--stacked flex-column': stacked }">
    <v-avatar
      :image="logoSrc"
      :size="size"
      :class="stacked ? 'mb-4' : 'mr-3'" />
    <span :class="['brand-mark__wordmark', 'text-primary', wordmarkClass]">Pkgly</span>
  </router-link>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    size?: number;
    wordmarkClass?: string;
    stacked?: boolean;
  }>(),
  {
    size: 40,
    wordmarkClass: "text-h6 font-weight-medium",
    stacked: false,
  },
);

const logoSrc = "/logo.svg";
</script>
