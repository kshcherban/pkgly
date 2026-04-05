<template>
  <v-text-field
    :id="id"
    type="number"
    v-model="value"
    v-bind="$attrs"
    variant="outlined"
    density="comfortable">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-text-field>
</template>
<script setup lang="ts">
defineProps({
  id: String,
});

const value = defineModel<number>({
  required: true,
});
</script>

<style scoped lang="scss">
/* Vuetify handles styling */
</style>
