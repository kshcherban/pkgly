<template>
  <section class="password-section">
    <v-text-field
      :id="id"
      :type="showPassword ? 'text' : 'password'"
      autocomplete="new-password"
      v-model="internalValue.value"
      variant="outlined"
      density="comfortable"
      :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
      @click:append-inner="showPassword = !showPassword"
      v-bind="$attrs"
      @focus="isFocused = true"
      @blur="isFocused = false">
      <template v-if="$slots.default" #label>
        <slot />
      </template>
    </v-text-field>
    <InputRequirements
      :show="isFocused"
      :validations="validations"
      :results="validationResults" />
  </section>

  <section class="password-section">
    <v-text-field
      :id="id + '-confirm'"
      :type="showConfirmPassword ? 'text' : 'password'"
      autocomplete="new-password"
      v-model="internalValue.confirmValue"
      variant="outlined"
      density="comfortable"
      :append-inner-icon="showConfirmPassword ? 'mdi-eye-off' : 'mdi-eye'"
      @click:append-inner="showConfirmPassword = !showConfirmPassword"
      :error="showMismatchError"
      :hint="undefined"
      :persistent-hint="false"
      v-bind="$attrs">
      <template #label>Confirm Password</template>
    </v-text-field>
    <div
      v-if="passwordsMatchMessage"
      class="password-status"
      :data-valid="passwordsMatch">
      <font-awesome-icon :icon="passwordsMatchMessage.icon" />
      <span>{{ passwordsMatchMessage.message }}</span>
    </div>
  </section>
</template>
<script setup lang="ts">
import type { PasswordRules } from "@/types/base";
import { computed, ref, watch, type PropType, type Ref } from "vue";
import InputRequirements from "./InputRequirements.vue";
import { siteStore } from "@/stores/site";
import { checkValidations, passwordValidationRules, type SyncValidationType } from "./validations";

const props = defineProps({
  id: {
    type: String,
    required: true,
  },

  passwordRules: {
    type: Object as PropType<PasswordRules>,
    required: false,
  },
});
const actualPasswordRules = computed(() => {
  if (props.passwordRules) {
    return props.passwordRules;
  }
  const site = siteStore();
  return site.getPasswordRulesOrDefault();
});
const passwordsMatch = ref(false);
const isFocused = ref(false);
const isValid = ref(false);
const showPassword = ref(false);
const showConfirmPassword = ref(false);
const passwordsMatchMessage = computed(() => {
  if (internalValue.value.value === "" && internalValue.value.confirmValue === "") {
    return undefined;
  }
  return passwordsMatch.value
    ? {
        message: "Passwords Match",
        icon: "fa-solid fa-circle-check",
      }
    : {
        message: "Passwords do not match",
        icon: "fa-solid fa-circle-xmark",
      };
});
const internalValue = ref({
  value: "",
  confirmValue: "",
});

const validationResults = ref<Record<string, boolean>>({});
const value = defineModel<string | undefined>({
  required: true,
});
watch(
  value,
  (newValue) => {
    internalValue.value = {
      value: newValue || "",
      confirmValue: newValue || "",
    };
  },
  { immediate: true },
);

const validations: Ref<Array<SyncValidationType>> = ref(
  passwordValidationRules(actualPasswordRules.value),
);

watch(
  internalValue,
  async (newValue) => {
    if (newValue.value !== newValue.confirmValue) {
      passwordsMatch.value = false;
    } else {
      passwordsMatch.value = true;
    }
    console.log(validations.value);
    const { isValid: newIsValid, validationResults: newValidationResults } = await checkValidations(
      validations.value,
      internalValue.value.value,
    );
    validationResults.value = newValidationResults;
    isValid.value = newIsValid;

    if (value.value === newValue.value) {
      return;
    }
    if (newIsValid && passwordsMatch.value) {
      console.log("Setting value");
      value.value = newValue.value;
    } else {
      console.log("Setting value to undefined");
      value.value = undefined;
    }
  },
  { deep: true },
);

const showMismatchError = computed(
  () =>
    !passwordsMatch.value &&
    internalValue.value.confirmValue.length > 0 &&
    internalValue.value.value.length > 0,
);
</script>
<style scoped lang="scss">
.password-section {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.password-status {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.875rem;
  &[data-valid="true"] {
    color: rgb(var(--v-theme-success));
  }
  &[data-valid="false"] {
    color: rgb(var(--v-theme-error));
  }
}
</style>
