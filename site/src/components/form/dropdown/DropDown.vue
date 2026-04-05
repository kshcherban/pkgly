<template>
  <v-select
    :id="actualId"
    :items="props.options"
    item-title="label"
    item-value="value"
    v-model="value"
    variant="outlined"
    density="comfortable">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-select>
</template>

<script setup lang="ts">
import { computed, type PropType } from "vue";

const props = defineProps({
  options: {
    type: Array as PropType<{ label: string; value: string }[]>,
    required: true,
  },
  id: {
    type: String,
    required: false,
  },
});

const actualId = computed(() => props.id ?? "dropdown");

const value = defineModel<string>({
  required: true,
});
</script>

<style lang="scss" scoped>
/* Vuetify handles styling */
</style>
