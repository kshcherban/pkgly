<!-- ABOUTME: Reusable empty-state placeholder: icon + title + message + optional action. -->
<!-- ABOUTME: Gives empty collections a consistent, guided, on-brand appearance. -->
<template>
  <div class="empty-state" role="status">
    <v-icon :icon="icon" size="64" color="medium-emphasis" class="empty-state__icon" />
    <h2 class="empty-state__title text-h6 text-medium-emphasis">{{ title }}</h2>
    <p v-if="message" class="empty-state__message text-body-2 text-medium-emphasis">
      {{ message }}
    </p>
    <div v-if="$slots.action" class="empty-state__action">
      <slot name="action" />
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  icon: string;
  title: string;
  message?: string;
}>();
</script>

<style scoped lang="scss">
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  gap: var(--nr-spacing-sm);
  padding: var(--nr-spacing-2xl) var(--nr-spacing-md);
  border: 1px dashed var(--nr-border-color-strong);
  border-radius: var(--nr-radius-lg);
}

.empty-state__icon {
  opacity: 0.8;
}

.empty-state__title {
  margin: var(--nr-spacing-sm) 0 0;
}

.empty-state__message {
  max-width: 32rem;
}

.empty-state__action {
  margin-top: var(--nr-spacing-md);
}
</style>
