<template>
  <section
    :id="id + '-section'"
    class="validatable-text-field"
    :data-valid="isValid">
    <v-text-field
      :id="id"
      :type="type ?? 'text'"
      v-model="internalValue"
      variant="outlined"
      density="comfortable"
      :error="showError"
      v-bind="$attrs"
      @focus="isFocused = true"
      @blur="isFocused = false"
      @keydown="handleDeniedKey">
      <template v-if="$slots.default" #label>
        <slot />
      </template>
    </v-text-field>
    <InputRequirements
      :show="isFocused"
      :validations="validations"
      :results="validationResults" />
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch, type PropType } from "vue";
import { checkValidations, type KeyPressAction, type ValidationType } from "./validations";
import InputRequirements from "./InputRequirements.vue";

const props = defineProps({
  id: String,
  originalValue: {
    type: String,
    required: false,
  },
  type: {
    type: String,
    required: false,
  },
  validations: {
    type: Array as PropType<ValidationType[]>,
    required: true,
  },
  deniedKeys: {
    type: Array as PropType<KeyPressAction[]>,
    required: false,
  },
});

const isFocused = ref(false);
const validationResults = ref<Record<string, boolean>>({});

const internalValue = ref(props.originalValue ?? "");
const isValid = ref(false);
const value = defineModel<string | undefined>({
  required: true,
});

const emit = defineEmits<{
  (e: "validity", valid: boolean): void;
}>();

watch(internalValue, async () => {
  const { isValid: newIsValid, validationResults: newValidationResults } = await checkValidations(
    props.validations,
    internalValue.value,
    props.originalValue,
  );
  validationResults.value = newValidationResults;

  if (newIsValid) {
    value.value = internalValue.value;
  } else {
    value.value = undefined;
  }
  isValid.value = newIsValid;
  emit("validity", newIsValid);
}, { immediate: true });

if (props.originalValue) {
  internalValue.value = props.originalValue;
  for (const validation of props.validations) {
    validationResults.value[validation.id] = true;
  }

  isValid.value = true;
  emit("validity", true);
}

const showError = computed(() => !isValid.value && internalValue.value.length > 0);

function handleDeniedKey(event: KeyboardEvent) {
  if (!props.deniedKeys) {
    return;
  }
  for (const deniedKey of props.deniedKeys) {
    if (typeof deniedKey === "string") {
      if (event.key === deniedKey) {
        event.preventDefault();
        return;
      }
    } else if (deniedKey.badKey === event.key) {
      event.preventDefault();
      internalValue.value += deniedKey.replacedKey;
      return;
    }
  }
}
</script>
<style scoped lang="scss">
.validatable-text-field {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
</style>
