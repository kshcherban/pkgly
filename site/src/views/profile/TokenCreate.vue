<template>
  <v-container class="py-6">
    <v-card v-if="!newResponseTokenResponse">
      <v-card-title class="d-flex align-center justify-space-between">
        <div>
          <div class="text-h6">Create Access Token</div>
          <div class="text-body-2 text-medium-emphasis">
            Generate a personal token for CLI and automation workflows.
          </div>
        </div>
      </v-card-title>

      <v-card-text>
        <v-form @submit.prevent="createToken">
          <v-row dense>
            <v-col cols="12" md="6">
              <TextInput
                id="tokenName"
                v-model="newToken.tokenName"
                required>
                Token Name
              </TextInput>
            </v-col>
            <v-col cols="12" md="6">
              <TextInput
                id="tokenDescription"
                v-model="newToken.tokenDescription">
                Description
              </TextInput>
            </v-col>
            <v-col cols="12">
              <fieldset class="expiration-options">
                <legend class="text-subtitle-2 font-weight-medium">Expiration</legend>
                <div class="expiration-options__presets">
                  <button
                    type="button"
                    class="expiration-option"
                    :class="{ 'expiration-option--active': expirationMode === 'never' }"
                    data-testid="expiration-never"
                    @click="selectExpiration('never')">
                    Never
                  </button>
                  <button
                    v-for="preset in expirationPresets"
                    :key="preset.days"
                    type="button"
                    class="expiration-option"
                    :class="{ 'expiration-option--active': expirationMode === preset.days }"
                    :data-testid="`expiration-preset-${preset.days}`"
                    @click="selectExpiration(preset.days)">
                    {{ preset.label }}
                  </button>
                  <button
                    type="button"
                    class="expiration-option"
                    :class="{ 'expiration-option--active': expirationMode === 'custom' }"
                    data-testid="expiration-custom"
                    @click="selectExpiration('custom')">
                    Custom
                  </button>
                </div>
                <TextInput
                  id="customExpirationDays"
                  v-model="customExpirationDays"
                  type="number"
                  min="1"
                  :disabled="expirationMode !== 'custom'"
                  placeholder="Days">
                  Custom days
                </TextInput>
              </fieldset>
            </v-col>
          </v-row>

          <v-divider class="my-4" />

          <section class="mt-4">
            <header class="section-header">
              <div>
                <div class="text-subtitle-1 font-weight-medium">Repository Scopes</div>
                <div class="text-body-2 text-medium-emphasis">
                  Limit this token to specific repositories and actions.
                </div>
              </div>
            </header>
            <RepositoryToActionList v-model="repositoryScopes" />
          </section>

          <v-divider class="my-4" />

          <section class="mt-4">
            <header class="section-header">
              <div>
                <div class="text-subtitle-1 font-weight-medium">Role Scopes</div>
                <div class="text-body-2 text-medium-emphasis">
                  Optional platform-wide scopes such as admin or read-only access.
                </div>
              </div>
            </header>
            <ScopesSelector v-model="scopes" />
          </section>

          <div class="token-create__actions mt-6">
            <SubmitButton
              color="primary"
              :block="false"
              :loading="isSubmitting"
              :disabled="isSubmitting"
              prepend-icon="mdi-plus">
              <span v-if="isSubmitting">Creating…</span>
              <span v-else>Create Token</span>
            </SubmitButton>
          </div>
        </v-form>
      </v-card-text>
    </v-card>

    <v-card
      v-else
      class="token-result-card">
      <v-card-title>
        <div>
          <div class="text-h6">Token Created</div>
          <div class="text-body-2 text-medium-emphasis">
            Copy and store this token securely. You will not be able to view it again.
          </div>
        </div>
      </v-card-title>
      <v-card-text>
        <div class="one-time-token">
          <div class="one-time-token__header">
            <div>
              <div class="text-subtitle-2 font-weight-medium">One-time token</div>
              <div class="text-body-2 text-medium-emphasis">
                {{ tokenExpirationSummary }}
              </div>
            </div>
            <v-btn
              color="primary"
              variant="tonal"
              data-testid="copy-token-button"
              prepend-icon="mdi-content-copy"
              @click="copyToken">
              Copy
            </v-btn>
          </div>
          <code
            class="one-time-token__secret"
            data-testid="token-output">{{ newResponseTokenResponse.token }}</code>
          <v-alert
            type="warning"
            variant="tonal"
            class="mt-4">
            Store this secret now. You will not be able to view it again.
          </v-alert>
        </div>
        <div class="d-flex justify-end mt-4">
          <v-btn
            color="primary"
            variant="flat"
            prepend-icon="mdi-key-plus"
            @click="resetForm">
            Create Another Token
          </v-btn>
        </div>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import ScopesSelector from "@/components/form/ScopesSelector.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import RepositoryToActionList from "@/components/nr/repository/RepositoryToActionList.vue";
import http from "@/http";
import type { ScopeDescription } from "@/types/base";
import type { RepositoryToActions } from "@/types/repository";
import { type NewAuthTokenResponse } from "@/types/user/token";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref } from "vue";

const newToken = ref({
  tokenName: "",
  tokenDescription: "",
});
const expirationPresets = [
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 90, label: "90 days" },
] as const;
const expirationMode = ref<"never" | "custom" | 7 | 30 | 90>("never");
const customExpirationDays = ref("");
const isSubmitting = ref(false);
const newResponseTokenResponse = ref<NewAuthTokenResponse | undefined>(undefined);
const repositoryScopes = ref<Array<RepositoryToActions>>([]);
const scopes = ref<Array<ScopeDescription>>([]);
const alerts = useAlertsStore();

const tokenExpirationSummary = computed(() => {
  const expiresAt = newResponseTokenResponse.value?.expires_at;
  if (!expiresAt) {
    return "Never expires";
  }
  return `Expires ${formatDateTime(expiresAt)}`;
});

function resetForm() {
  newResponseTokenResponse.value = undefined;
  newToken.value = {
    tokenName: "",
    tokenDescription: "",
  };
  expirationMode.value = "never";
  customExpirationDays.value = "";
  repositoryScopes.value = [];
  scopes.value = [];
}

function selectExpiration(mode: "never" | "custom" | 7 | 30 | 90) {
  expirationMode.value = mode;
}

function resolveExpiresInDays(): number | null | undefined {
  if (expirationMode.value === "never") {
    return null;
  }
  if (expirationMode.value !== "custom") {
    return expirationMode.value;
  }
  if (!/^\d+$/.test(customExpirationDays.value)) {
    return undefined;
  }
  const days = Number(customExpirationDays.value);
  if (!Number.isSafeInteger(days) || days <= 0) {
    return undefined;
  }
  return days;
}

function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString();
}

async function copyToken() {
  const token = newResponseTokenResponse.value?.token;
  if (!token) {
    return;
  }
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(token);
    } else {
      fallbackCopyToken(token);
    }
    alerts.success("Token copied", "The token was copied to the clipboard.");
  } catch (error) {
    console.error(error);
    alerts.error("Copy failed", "Copy the token manually from the token panel.");
  }
}

function fallbackCopyToken(token: string) {
  const input = document.createElement("textarea");
  input.value = token;
  input.setAttribute("readonly", "true");
  input.style.position = "fixed";
  input.style.left = "-9999px";
  document.body.appendChild(input);
  input.select();
  document.execCommand("copy");
  document.body.removeChild(input);
}

async function createToken() {
  if (isSubmitting.value) {
    return;
  }
  const expiresInDays = resolveExpiresInDays();
  if (expiresInDays === undefined) {
    alerts.error(
      "Invalid expiration",
      "Custom expiration must be a positive whole number of days.",
    );
    return;
  }
  isSubmitting.value = true;
  try {
    const repositoryScopesRequest = repositoryScopes.value.map((repositoryScope) => ({
      repository_id: repositoryScope.repositoryId,
      scopes: repositoryScope.actions.asArray(),
    }));

    const scopesRequest = scopes.value.map((scope) => scope.key);

    const request = {
      name: newToken.value.tokenName,
      description: newToken.value.tokenDescription,
      expires_in_days: expiresInDays,
      repository_scopes: repositoryScopesRequest,
      scopes: scopesRequest,
    };

    const response = await http.post<NewAuthTokenResponse>("/api/user/token/create", request);
    newResponseTokenResponse.value = response.data;
    alerts.success("Token created", "Copy the token now. It will not be shown again.");
  } catch (error) {
    console.error(error);
    alerts.error("Error creating token", "An error occurred while creating the token.");
  } finally {
    isSubmitting.value = false;
  }
}
</script>

<style scoped lang="scss">
.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
}

.token-result-card {
  text-align: left;
}

.token-create__actions {
  display: flex;
  justify-content: flex-start;
}

.expiration-options {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 8px;
  padding: 0.75rem;
}

.expiration-options__presets {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin: 0.5rem 0 0.75rem;
}

.expiration-option {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
  min-height: 2.25rem;
  padding: 0 0.85rem;
  font-weight: 500;
}

.expiration-option--active {
  background: rgb(var(--v-theme-primary));
  border-color: rgb(var(--v-theme-primary));
  color: rgb(var(--v-theme-on-primary));
}

.one-time-token {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 8px;
  padding: 1rem;
}

.one-time-token__header {
  align-items: flex-start;
  display: flex;
  gap: 1rem;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.one-time-token__secret {
  background: rgba(0, 0, 0, 0.04);
  border-radius: 6px;
  display: block;
  font-family: var(--nr-font-family-mono, "Roboto Mono", monospace);
  overflow-wrap: anywhere;
  padding: 0.75rem;
  white-space: pre-wrap;
}

@media (max-width: 768px) {
  .one-time-token__header {
    flex-direction: column;
  }
}
</style>
