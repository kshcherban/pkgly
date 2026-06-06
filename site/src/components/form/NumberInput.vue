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
import { watch } from "vue";

defineProps({
  id: String,
});

const value = defineModel<number>({
  required: true,
});

watch(value, (val) => {
  const raw = val as unknown;
  if (typeof raw === "string") {
    const trimmed = raw.trim();
    if (trimmed === "") {
      return;
    }
    const num = Number(trimmed);
    if (Number.isFinite(num)) {
      value.value = num;
    }
  }
});
</script>

<style scoped lang="scss">
/* Vuetify handles styling */
</style>
