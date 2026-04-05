<template>
  <div class="datePicker">
    <label :for="id">
      <slot />
    </label>
    <VueDatePicker
      v-model="value"
      :id="id"
      :enable-time-picker="false">
      Received Date
    </VueDatePicker>
  </div>
</template>
<script setup lang="ts">
defineProps<{
  id: string;
}>();
const value = defineModel<Date>({
  required: true,
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/variables.scss" as *;
</style>
