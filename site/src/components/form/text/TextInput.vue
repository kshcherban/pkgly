<template>
  <v-text-field
    :id="id"
    :type="type"
    :label="label"
    :error-messages="error"
    :clearable="haveClearButton"
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
import { computed } from 'vue';

const props = defineProps({
  id: String,
  haveClearButton: {
    type: Boolean,
    default: false,
  },
  error: {
    type: String,
    required: false,
  },
  type: {
    type: String,
    default: "text",
  },
  label: {
    type: String,
    required: false,
  },
});

const value = defineModel<string | undefined>({
  required: true,
});
</script>

<style scoped lang="scss">
/* Vuetify handles styling */
</style>
