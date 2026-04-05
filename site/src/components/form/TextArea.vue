<template>
  <v-textarea
    :id="id"
    v-model="value"
    v-bind="$attrs"
    variant="outlined"
    density="comfortable">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-textarea>
</template>
<script setup lang="ts">
defineProps<{
  id: string;
}>();

const value = defineModel<string | undefined>({
  required: true,
});
</script>

<style scoped lang="scss">
/* Vuetify handles styling */
</style>
