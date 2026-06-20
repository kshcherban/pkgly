<!-- ABOUTME: Unified repository status badges (Secured/Unsecured + Active/Inactive). -->
<!-- ABOUTME: Single styling source for status indicators across home, detail, and admin. -->
<template>
  <span class="status-chips" role="group" aria-label="Repository status">
    <span
      class="status-chip"
      :class="secured ? 'status-chip--secured' : 'status-chip--neutral'"
      data-testid="status-secured">
      {{ secured ? "Secured" : "Unsecured" }}
    </span>
    <span
      class="status-chip"
      :class="active ? 'status-chip--active' : 'status-chip--neutral'"
      data-testid="status-active">
      {{ active ? "Active" : "Inactive" }}
    </span>
  </span>
</template>

<script setup lang="ts">
defineProps<{
  secured: boolean;
  active: boolean;
}>();
</script>

<style scoped lang="scss">
.status-chips {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--nr-spacing-xs);
}

.status-chip {
  display: inline-flex;
  align-items: center;
  padding: 0.125rem 0.5rem;
  border-radius: 999px;
  font-size: var(--nr-font-size-xs);
  font-weight: var(--nr-font-weight-medium);
  line-height: 1.4;
  white-space: nowrap;
  border: 1px solid transparent;
}

.status-chip--secured {
  background: var(--nr-badge-secure-bg);
  color: var(--nr-badge-secure-fg);
  border-color: var(--nr-badge-secure-bg);
}

.status-chip--active {
  background: var(--nr-badge-success-bg);
  color: var(--nr-badge-success-fg);
  border-color: var(--nr-badge-success-bg);
}

.status-chip--neutral {
  background: var(--nr-badge-neutral-bg);
  color: var(--nr-badge-neutral-fg);
  border-color: var(--nr-border-color);
}
</style>
