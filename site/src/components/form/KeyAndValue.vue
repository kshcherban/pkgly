<template>
  <div class="keyValue">
    <span class="spanAsLabel">{{ label }}</span>
    <span class="value">{{ value }}</span>
  </div>
</template>

<script setup lang="ts">
defineProps({
  label: {
    type: String,
    required: true,
  },
  value: {
    type: String,
    required: true,
  },
});
</script>
<style lang="scss" scoped>
@use "@/assets/styles/theme" as *;

.keyValue {
  display: flex;
  flex-direction: column;
  margin-bottom: 10px;
  width: 100%;
  .value {
    display: block;
    border: 2px solid $secondary-50;
    padding: 5px;
    border-radius: 5px;
    background-color: $background;
    color: $text;
  }
  // Make it look like a input
  label {
    font-weight: bold;
  }
}
</style>
