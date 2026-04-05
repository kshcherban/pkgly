<template>
  <v-container class="py-6">
    <v-card>
      <v-card-title class="d-flex align-center justify-space-between">
        <div>
          <div class="text-h6">Access Tokens</div>
          <div class="text-body-2 text-medium-emphasis">
            Manage personal tokens used for CI, automation, and API access.
          </div>
        </div>
        <v-btn
          color="primary"
          variant="flat"
          prepend-icon="mdi-key-plus"
          :to="{ name: 'profileTokenCreate' }">
          New Token
        </v-btn>
      </v-card-title>

      <v-card-text>
        <v-progress-circular
          v-if="loading"
          indeterminate
          color="primary"
          size="40" />

        <v-alert
          v-else-if="error"
          type="error"
          variant="tonal"
          class="mb-4">
          {{ error }}
        </v-alert>

        <v-expansion-panels
          v-else-if="authTokens.length > 0"
          multiple
          class="token-panels">
          <v-expansion-panel
            v-for="token in authTokens"
            :key="token.token.id">
            <v-expansion-panel-title>
              <div class="panel-title">
                <div class="panel-title__primary">
                  <span class="panel-title__name">{{ token.token.name || "Untitled Token" }}</span>
                  <span class="panel-title__source">{{ token.token.source }}</span>
                </div>
                <div class="panel-title__meta">
                  <span>Created {{ formatDate(token.token.created_at) }}</span>
                  <v-chip
                    :color="token.token.active ? 'success' : 'error'"
                    :variant="token.token.active ? 'flat' : 'outlined'"
                    size="small">
                    {{ token.token.active ? "Active" : "Revoked" }}
                  </v-chip>
                </div>
              </div>
            </v-expansion-panel-title>
            <v-expansion-panel-text>
              <div class="panel-body">
                <div class="panel-body__row">
                  <span class="panel-body__label">Token ID</span>
                  <code class="panel-body__value">{{ token.token.id }}</code>
                </div>
                <div class="panel-body__row">
                  <span class="panel-body__label">Created</span>
                  <span class="panel-body__value">{{ formatDateTime(token.token.created_at) }}</span>
                </div>
                <div class="d-flex justify-end mt-4">
                  <v-btn
                    color="error"
                    variant="tonal"
                    prepend-icon="mdi-delete"
                    data-testid="token-delete-button"
                    @click.stop="deleteToken(token.token.id)">
                    Delete Token
                  </v-btn>
                </div>
              </div>
            </v-expansion-panel-text>
          </v-expansion-panel>
        </v-expansion-panels>

        <v-card
          v-else
          variant="outlined"
          class="text-center py-8">
          <div class="text-h6 text-medium-emphasis mb-2">No tokens yet</div>
          <div class="text-body-2 text-medium-emphasis mb-4">
            Generate a token to authenticate CLI and automation workflows.
          </div>
          <v-btn
            color="primary"
            variant="flat"
            prepend-icon="mdi-key-plus"
            :to="{ name: 'profileTokenCreate' }">
            Create Token
          </v-btn>
        </v-card>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import http from "@/http";
import { sessionStore } from "@/stores/session";
import { type RawAuthTokenFullResponse } from "@/types/user/token";
import { useAlertsStore } from "@/stores/alerts";
import { ref } from "vue";

const session = sessionStore();
const user = session.user;
const authTokens = ref<Array<RawAuthTokenFullResponse>>([]);
const loading = ref(true);
const error = ref<string | null>(null);
const alerts = useAlertsStore();

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString();
}

function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString();
}

async function deleteToken(id: number) {
  try {
    await http.delete(`/api/user/token/delete/${id}`);
    alerts.success("Token deleted", "The token was removed successfully.");
    await getAuthTokens();
  } catch (err) {
    console.error(err);
    alerts.error("Failed to delete token", "An unexpected error occurred.");
  }
}

async function getAuthTokens() {
  if (!user) {
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    const response = await http.get<Array<RawAuthTokenFullResponse>>("/api/user/token/list");
    authTokens.value = response.data.filter((token) => !isDockerBearerToken(token));
  } catch (err) {
    console.error(err);
    error.value = "Unable to load tokens. Please try again.";
  } finally {
    loading.value = false;
  }
}

void getAuthTokens();

function isDockerBearerToken(token: RawAuthTokenFullResponse): boolean {
  const source = token.token.source?.toLowerCase?.() ?? "";
  return source.includes("docker");
}
</script>

<style scoped lang="scss">
.token-panels {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 8px;
}

.panel-title {
  display: flex;
  justify-content: space-between;
  width: 100%;
  gap: 1rem;
  text-align: left;
}

.panel-title__primary {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.panel-title__name {
  font-weight: 600;
}

.panel-title__source {
  font-size: 0.9rem;
  color: var(--nr-text-secondary, rgba(0, 0, 0, 0.6));
}

.panel-title__meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  font-size: 0.9rem;
  color: var(--nr-text-secondary, rgba(0, 0, 0, 0.6));
}

.panel-body {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.panel-body__row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  font-size: 0.95rem;
}

.panel-body__label {
  font-weight: 500;
  color: var(--nr-text-secondary, rgba(0, 0, 0, 0.6));
}

.panel-body__value {
  font-family: var(--nr-font-family-mono, "Roboto Mono", monospace);
}

@media (max-width: 768px) {
  .panel-title {
    flex-direction: column;
    align-items: flex-start;
  }

  .panel-title__meta {
    flex-wrap: wrap;
  }

  .panel-body__row {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
