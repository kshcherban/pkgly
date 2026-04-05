<template>
  <img
    class="cargo-icon"
    :src="rustLogo"
    alt="Rust logo" />
</template>

<script setup lang="ts">
import rustLogo from "@/assets/icons/rust-logo-512x512.png";
</script>

<style scoped lang="scss">
.cargo-icon {
  width: 40px;
  height: 40px;
  object-fit: contain;
  border-radius: 8px;
  background: #fff;
  padding: 4px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}
</style>
