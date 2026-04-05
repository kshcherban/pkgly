<template>
  <v-text-field
    :id="id"
    :type="showPassword ? 'text' : 'password'"
    v-model="value"
    v-bind="$attrs"
    autocomplete="current-password"
    variant="outlined"
    density="comfortable"
    :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
    @click:append-inner="showPassword = !showPassword">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-text-field>
</template>
<script setup lang="ts">
import { ref } from "vue";

defineProps({
  id: {
    type: String,
    required: true,
  },
});

const value = defineModel<string>({
  required: true,
});

const showPassword = ref(false);
</script>

<style scoped lang="scss">
/* Vuetify handles styling */
</style>
