<template>
  <main class="systemSettings">
    <h1>Password Rules</h1>

    <section class="card">
      <header>
        <h2>Password strength requirements</h2>
        <p>
          When enabled, these rules apply to all user passwords
          (login, registration, password changes).
        </p>
      </header>

      <v-switch
        v-model="enabled"
        color="primary"
        label="Enable password rules"
        hide-details
        density="comfortable" />

      <template v-if="enabled">
        <v-divider />

        <div class="grid">
          <v-text-field
            v-model.number="rules.min_length"
            label="Minimum password length"
            type="number"
            min="0"
            max="256"
            variant="outlined"
            density="comfortable"
            hide-details="auto" />
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
    </section>

    <div class="actions">
      <v-btn
        variant="text"
        color="secondary"
        :disabled="saving"
        @click="resetForm">
        Reset
      </v-btn>
      <div class="actions__spacer" />
      <v-btn
        color="primary"
        variant="flat"
        :loading="saving"
        :disabled="!canSave"
        @click="save">
        Save Changes
      </v-btn>
    </div>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import http from "@/http";
import { siteStore } from "@/stores/site";
import type { PasswordRules } from "@/types/base";
import { useAlertsStore } from "@/stores/alerts";
import { resolveRequestError } from "./resolveRequestError";

const alerts = useAlertsStore();
const enabled = ref(true);
const rules = reactive<PasswordRules>({
  min_length: 8,
  require_uppercase: true,
  require_lowercase: true,
  require_number: true,
  require_symbol: true,
});
const defaults = reactive<PasswordRules>({ ...rules });
const saving = ref(false);
const loaded = ref(false);

onMounted(async () => {
  try {
    const response = await http.get<PasswordRules | null>("/api/security/password-rules");
    if (response.data) {
      Object.assign(rules, response.data);
      Object.assign(defaults, response.data);
      enabled.value = true;
    } else {
      enabled.value = false;
    }
    loaded.value = true;
  } catch (error) {
    const { title, message } = resolveRequestError(error, "Failed to load password rules", "");
    alerts.error(title, message);
  }
});

const canSave = computed(() => {
  if (!loaded.value || saving.value) return false;
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
  saving.value = true;
  try {
    const payload = enabled.value ? { ...rules } : null;
    await http.put("/api/security/password-rules", payload);
    Object.assign(defaults, rules);
    const site = siteStore();
    await site.getInfo();
    alerts.success("Password rules saved");
  } catch (error) {
    const { title, message } = resolveRequestError(
      error,
      "Failed to save password rules",
      "",
    );
    alerts.error(title, message);
  } finally {
    saving.value = false;
  }
}

function resetForm() {
  if (enabled.value) {
    Object.assign(rules, defaults);
  }
}
</script>

<style scoped lang="scss">
@use "./systemSettings.scss";
</style>
