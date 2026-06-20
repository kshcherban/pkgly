<!-- ABOUTME: Displays long hash/digest values truncated, with a copy-to-clipboard -->
<!-- ABOUTME: action and the full value available via tooltip for inspection. -->
<template>
  <span class="mono-value">
    <code :title="value">{{ displayValue }}</code>
    <button
      v-if="value && canCopy"
      type="button"
      class="mono-value__copy"
      data-testid="mono-copy"
      :aria-label="`Copy ${value}`"
      title="Copy to clipboard"
      @click="copy">
      <v-icon size="x-small" :icon="copied ? 'mdi-check' : 'mdi-content-copy'" />
    </button>
  </span>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { truncateMiddle } from "@/utils/truncateValue";
import { useAlertsStore } from "@/stores/alerts";

const props = defineProps<{ value: string }>();

const displayValue = computed(() => truncateMiddle(props.value));
const canCopy = computed(() => Boolean(navigator.clipboard?.writeText));
const copied = ref(false);
const alerts = useAlertsStore();

async function copy(): Promise<void> {
  if (!props.value) {
    return;
  }
  try {
    await navigator.clipboard.writeText(props.value);
    copied.value = true;
    window.setTimeout(() => {
      copied.value = false;
    }, 1500);
    alerts.success("Copied to clipboard");
  } catch {
    alerts.error("Failed to copy to clipboard");
  }
}
</script>

<style scoped lang="scss">
.mono-value {
  display: inline-flex;
  align-items: center;
  gap: var(--nr-spacing-xs);
  max-width: 100%;
}

code {
  font-family: var(--nr-font-family-mono);
  background: var(--nr-surface-variant);
  padding: 0.1rem 0.35rem;
  border-radius: var(--nr-radius-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.mono-value__copy {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  border: 1px solid var(--nr-border-color);
  border-radius: var(--nr-radius-md);
  background: var(--nr-background);
  color: var(--nr-text-secondary);
  cursor: pointer;
  transition: color var(--nr-transition-fast), border-color var(--nr-transition-fast);

  &:hover,
  &:focus-visible {
    color: var(--nr-primary);
    border-color: var(--nr-primary);
  }
}
</style>
