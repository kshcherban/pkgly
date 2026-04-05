<template>
  <div ref="wrapperRef" class="switch-wrapper" @click.capture="handleWrapperClick">
    <v-switch
      :id="id"
      v-model="value"
      :aria-label="ariaLabel"
      color="primary"
      hide-details>
      <template #label>
        <div
          v-if="!hideLabel"
          class="switch-label-content"
          @click="handleLabelClick">
          <span class="switch-label-text">
            <slot />
          </span>
          <span v-if="$slots.comment" class="switch-comment">
            <slot name="comment" />
          </span>
        </div>
      </template>
    </v-switch>
  </div>
</template>
<script setup lang="ts">
import { ref, watch } from "vue";

const props = defineProps({
  id: {
    type: String,
    required: true,
  },
  hideLabel: {
    type: Boolean,
    default: false,
  },
  ariaLabel: {
    type: String,
    default: undefined,
  },
});

const value = defineModel<boolean>({
  required: true,
});

const emit = defineEmits<{
  (e: "change", newValue: boolean): void;
}>();

// Add logging for debugging
watch(value, (newValue, oldValue) => {
  console.log(`SwitchInput [${props.id}] value changed:`, { oldValue, newValue });
});

const wrapperRef = ref<HTMLElement | null>(null);

function isInteractiveElement(target: Element): boolean {
  return !!target.closest(
    "a,button,input,textarea,select,summary,[role='button'],[role='link']",
  );
}

function handleLabelClick(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }

  // If the label contains interactive elements (links, buttons, etc.), prevent toggling.
  if (isInteractiveElement(target)) {
    event.stopPropagation();
  }
}

function handleWrapperClick(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) {
    return;
  }

  if (target.closest(".switch-label-content") && isInteractiveElement(target)) {
    return;
  }

  const input =
    wrapperRef.value?.querySelector<HTMLInputElement>("input[type='checkbox'],input[type='radio']") ??
    (document.getElementById(props.id) instanceof HTMLInputElement
      ? (document.getElementById(props.id) as HTMLInputElement)
      : null);
  if (!input) return;

  if (input.disabled || input.getAttribute("aria-disabled") === "true") {
    return;
  }

  const before = value.value;
  const beforeChecked = input.checked;
  // If something prevents the checkbox default action / v-model update, fall back to toggling
  // the underlying v-model manually.
  setTimeout(() => {
    // If the v-model updated, the switch will repaint normally.
    if (value.value !== before) {
      return;
    }

    // If the DOM input toggled but v-model did not (rare Vuetify failure mode), sync to the input.
    // This ensures the `v-selection-control--dirty` class updates so the knob/track move visually.
    if (input.checked !== beforeChecked) {
      value.value = input.checked;
      return;
    }

    // Otherwise, nothing toggled—force a v-model toggle.
    value.value = !before;
  }, 0);
}

watch(value, (newValue) => {
  emit("change", newValue);
});
</script>

<style scoped lang="scss">
.switch-wrapper {
  margin: 1rem 0;
  /* Ensure the wrapper is hit-testable even in visually empty areas. */
  background-color: rgba(0, 0, 0, 0);
}

.switch-label-content {
  display: flex;
  flex-direction: column;
}

.switch-label-text {
  font-size: 1rem;
  font-weight: 500;
  color: var(--nr-text-primary);
}

.switch-comment {
  font-size: 0.875rem;
  color: var(--nr-text-secondary);
  margin-top: 0.25rem;
}

/* Ensure smooth transitions for v-switch */
:deep(.v-switch) {
  transition: all 0.2s ease-in-out;

  .v-selection-control__input {
    transition: inherit;
  }

  .v-switch__thumb {
    transition: inherit;
  }

  .v-switch__track {
    transition: inherit;
  }
}
</style>
