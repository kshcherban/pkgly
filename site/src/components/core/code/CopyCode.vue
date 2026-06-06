<!-- ABOUTME: Displays a code value with a keyboard-accessible clipboard action. -->
<!-- ABOUTME: Reports successful copies through the shared alerts store. -->
<template>
  <div class="copyURL">
    <div v-if="$slots.default || label" class="copyURL__label">
      <slot>{{ label }}</slot>
    </div>
    <div class="copyURL__control">
      <code>{{ code }}</code>
      <button
        type="button"
        :aria-label="`Copy ${label}`"
        :title="`Copy ${label}`"
        @click="copyURL">
        <v-icon size="small" aria-hidden="true">mdi-content-copy</v-icon>
      </button>
    </div>
  </div>
</template>
<script setup lang="ts">
import { useAlertsStore } from "@/stores/alerts";

const props = defineProps({
  code: {
    type: String,
    required: true,
  },
  label: {
    type: String,
    default: "value",
  },
});

const alerts = useAlertsStore();
function copyURL() {
  navigator.clipboard.writeText(props.code);
  alerts.success("Copied");
}
</script>

<style lang="scss" scoped>
@use "@/assets/styles/theme.scss" as *;
.copyURL {
  min-width: 0;
}

.copyURL__label {
  margin-bottom: var(--nr-spacing-xs);
  color: var(--nr-text-secondary);
  font-size: var(--nr-font-size-sm);
}
@media screen and (max-width: 768px) {
  code {
    max-width: 90%;
    word-wrap: break-word;
  }
}

.copyURL__control {
  display: flex;
  max-width: 100%;
  align-items: stretch;
}

code {
  min-width: 0;
  overflow: hidden;
  padding: var(--nr-spacing-sm) var(--nr-spacing-md);
  border: 1px solid var(--nr-border-color);
  border-right: 0;
  border-radius: var(--nr-radius-md) 0 0 var(--nr-radius-md);
  background: var(--nr-surface-variant);
  font-family: var(--nr-font-family-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--nr-border-color);
  border-radius: 0 var(--nr-radius-md) var(--nr-radius-md) 0;
  background: var(--nr-background);
  color: var(--nr-primary);
  cursor: pointer;
  padding: 0 var(--nr-spacing-sm);

  &:hover,
  &:focus-visible {
    background: $primary-50;
    outline: none;
    box-shadow: var(--nr-focus-ring);
  }
}
</style>
