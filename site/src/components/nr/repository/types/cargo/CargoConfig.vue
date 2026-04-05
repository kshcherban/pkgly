<template>
  <div class="cargo-config">
    <v-alert
      border="start"
      type="info"
      variant="tonal"
      class="text-body-2">
      Cargo registries in Pkgly currently operate in hosted mode. No additional
      configuration is required.
    </v-alert>
  </div>
</template>

<script setup lang="ts">
defineModel<{ type: "Hosted" }>({
  default: { type: "Hosted" },
});
</script>

<style scoped lang="scss">
.cargo-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
</style>
