<template>
  <v-text-field
    :id="id"
    type="email"
    autocomplete="email"
    variant="outlined"
    density="comfortable"
    v-model="value"
    v-bind="$attrs">
    <template v-if="$slots.default" #label>
      <slot />
    </template>
  </v-text-field>
</template>

<script setup lang="ts">
defineProps<{
  id: string;
}>();

const value = defineModel<string>({
  required: true,
});
</script>
