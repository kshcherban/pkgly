<template>
  <main class="systemSettings">
    <FloatingErrorBanner
      :visible="errorBanner.visible"
      :title="errorBanner.title"
      :message="errorBanner.message"
      @close="resetError" />
    <h1>Password Rules</h1>

    <section class="card">
      <header>
        <h2>Password strength requirements</h2>
        <p>
          When enabled, these rules apply to all user passwords
          (login, registration, password changes).
        </p>
      </header>
      <SpinnerElement v-if="loading" />
      <form
        v-else
        class="passwordRulesForm"
        @submit.prevent="save">
        <SwitchInput
          id="password-rules-enabled"
          v-model="enabled">
          Enable password rules
          <template #comment>
            When disabled, users can set any password regardless of the constraints below.
          </template>
        </SwitchInput>

        <template v-if="enabled">
          <div class="grid">
            <NumberInput
              id="password-min-length"
              v-model="rules.min_length"
              min="0"
              max="256">
              Minimum password length
            </NumberInput>
          </div>

          <v-checkbox
            v-model="rules.require_uppercase"
            label="Require at least one uppercase letter"
            color="primary"
            density="comfortable"
            hide-details />

          <v-checkbox
            v-model="rules.require_lowercase"
            label="Require at least one lowercase letter"
            color="primary"
            density="comfortable"
            hide-details />

          <v-checkbox
            v-model="rules.require_number"
            label="Require at least one number"
            color="primary"
            density="comfortable"
            hide-details />

          <v-checkbox
            v-model="rules.require_symbol"
            label="Require at least one special character"
            color="primary"
            density="comfortable"
            hide-details />

          <v-alert
            v-if="hasNoConstraints"
            type="warning"
            variant="tonal"
            density="comfortable"
            text="At least one constraint field must be active." />
        </template>

        <footer class="actions">
          <SubmitButton
            :block="false"
            :disabled="!canSave || saving"
            :loading="saving"
            prepend-icon="mdi-content-save"
            title="Save password rules">
            Save
          </SubmitButton>
          <span class="actions__spacer" />
          <v-btn
            variant="outlined"
            color="primary"
            :disabled="saving"
            @click="resetForm">
            Reset
          </v-btn>
        </footer>
      </form>
    </section>
  </main>
</template>

<script setup lang="ts">
import SwitchInput from "@/components/form/SwitchInput.vue";
import NumberInput from "@/components/form/NumberInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SpinnerElement from "@/components/spinner/SpinnerElement.vue";
import FloatingErrorBanner from "@/components/ui/FloatingErrorBanner.vue";
import http from "@/http";
import { siteStore } from "@/stores/site";
import type { PasswordRules } from "@/types/base";
import { useAlertsStore } from "@/stores/alerts";
import { resolveRequestError } from "./resolveRequestError";
import { computed, onMounted, reactive, ref, watch } from "vue";

const alerts = useAlertsStore();
const loading = ref(true);
const saving = ref(false);
const enabled = ref(true);
const rules = reactive<PasswordRules>({
  min_length: 8,
  require_uppercase: true,
  require_lowercase: true,
  require_number: true,
  require_symbol: true,
});
const initialSignature = ref(JSON.stringify({ enabled: true, ...rules }));

const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});

const showError = (title: string, message: string) => {
  errorBanner.value.visible = true;
  errorBanner.value.title = title;
  errorBanner.value.message = message;
};

const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};

watch(
  [() => ({ ...rules }), enabled],
  () => {
    if (errorBanner.value.visible) {
      resetError();
    }
  },
  { deep: true },
);

onMounted(async () => {
  loading.value = true;
  resetError();
  try {
    const response = await http.get<PasswordRules | null>("/api/security/password-rules");
    if (response.data) {
      Object.assign(rules, response.data);
      enabled.value = true;
    } else {
      enabled.value = false;
    }
    initialSignature.value = JSON.stringify({ enabled: enabled.value, ...rules });
  } catch (error) {
    const resolved = resolveRequestError(
      error,
      "Unable to load password rules",
      "Check the server logs for more information.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    loading.value = false;
  }
});

const hasChanges = computed(
  () => initialSignature.value !== JSON.stringify({ enabled: enabled.value, ...rules }),
);

const canSave = computed(() => {
  if (saving.value) return false;
  if (!hasChanges.value) return false;
  if (!enabled.value) return true;
  return (
    rules.min_length > 0 ||
    rules.require_uppercase ||
    rules.require_lowercase ||
    rules.require_number ||
    rules.require_symbol
  );
});

const hasNoConstraints = computed(
  () =>
    enabled.value &&
    rules.min_length === 0 &&
    !rules.require_uppercase &&
    !rules.require_lowercase &&
    !rules.require_number &&
    !rules.require_symbol,
);

async function save() {
  if (saving.value) {
    return;
  }
  resetError();

  if (enabled.value && hasNoConstraints.value) {
    showError("No constraints active", "At least one constraint field must be active.");
    return;
  }

  saving.value = true;
  try {
    const payload = enabled.value ? { ...rules } : null;
    await http.put("/api/security/password-rules", payload);
    initialSignature.value = JSON.stringify({ enabled: enabled.value, ...rules });
    const site = siteStore();
    await site.getInfo();
    alerts.success("Password rules saved");
  } catch (error) {
    const resolved = resolveRequestError(
      error,
      "Unable to save password rules",
      "Check the server logs for more details.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    saving.value = false;
  }
}

function resetForm() {
  if (saving.value) {
    return;
  }
  resetError();
  const snapshot = JSON.parse(initialSignature.value) as {
    enabled: boolean;
    min_length: number;
    require_uppercase: boolean;
    require_lowercase: boolean;
    require_number: boolean;
    require_symbol: boolean;
  };
  enabled.value = snapshot.enabled;
  Object.assign(rules, {
    min_length: snapshot.min_length,
    require_uppercase: snapshot.require_uppercase,
    require_lowercase: snapshot.require_lowercase,
    require_number: snapshot.require_number,
    require_symbol: snapshot.require_symbol,
  });
}
</script>

<style scoped lang="scss">
@use "./systemSettings.scss";

.passwordRulesForm {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}
</style>
