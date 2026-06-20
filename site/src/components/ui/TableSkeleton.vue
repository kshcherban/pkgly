<!-- ABOUTME: Reusable loading placeholder that mimics table rows for fetch states. -->
<!-- ABOUTME: Built on Vuetify's skeleton loader so styling stays consistent. -->
<template>
  <div class="table-skeleton" role="status" aria-label="Loading data">
    <v-skeleton-loader
      v-for="row in rows"
      :key="row"
      type="table-row"
      class="table-skeleton__row" />
  </div>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ rows?: number }>(), { rows: 5 });
</script>

<style scoped lang="scss">
.table-skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--nr-spacing-xs);
  padding: var(--nr-spacing-md);
}

.table-skeleton__row {
  border-radius: var(--nr-radius-md);
}
</style>
