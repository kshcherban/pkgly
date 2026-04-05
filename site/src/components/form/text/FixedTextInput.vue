<template>
  <v-text-field
    :id="id"
    :model-value="value ?? ''"
    readonly
    variant="outlined"
    density="comfortable"
    :hide-details="hideDetails">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-text-field>
</template>

<script setup lang="ts">
defineProps<{
  id?: string;
  value?: string | number | null;
  hideDetails?: boolean;
}>();
</script>
