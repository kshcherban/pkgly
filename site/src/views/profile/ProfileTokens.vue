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
          v-if="authTokens.length > 0"
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

        <div
          v-else-if="authTokens.length > 0"
          class="token-list">
          <div
            v-for="token in authTokens"
            :key="token.token.id"
            class="token-row"
            data-testid="token-row">
            <div class="token-row__main">
              <div class="token-row__identity">
                <div class="token-row__name">{{ token.token.name || "Untitled Token" }}</div>
                <div class="token-row__description">
                  {{ token.token.description || token.token.source }}
                </div>
              </div>
              <div class="token-row__meta">
                <v-chip
                  :color="tokenStatus(token).color"
                  :variant="tokenStatus(token).variant"
                  size="small">
                  {{ tokenStatus(token).label }}
                </v-chip>
                <span>Created {{ formatDate(token.token.created_at) }}</span>
                <span>{{ formatExpiration(token.token.expires_at) }}</span>
              </div>
            </div>

            <div class="token-row__scopes">
              <div class="token-scope-group">
                <div class="token-scope-group__label">Role scopes</div>
                <div class="token-scope-group__values">
                  <span
                    v-for="scope in token.scopes"
                    :key="scope.id"
                    class="scope-pill">
                    {{ roleScopeName(scope.scope) }}
                  </span>
                  <span
                    v-if="token.scopes.length === 0"
                    class="scope-empty">None</span>
                </div>
              </div>

              <div class="token-scope-group">
                <div class="token-scope-group__label">Repository scopes</div>
                <div class="token-scope-group__values token-scope-group__values--stacked">
                  <span
                    v-for="repositoryScope in token.repository_scopes"
                    :key="repositoryScope.id"
                    class="repo-scope">
                    <span class="repo-scope__name">
                      {{ repositoryName(repositoryScope.repository_id) }}
                    </span>
                    <span class="repo-scope__actions">
                      {{ repositoryScope.actions.join(", ") }}
                    </span>
                  </span>
                  <span
                    v-if="token.repository_scopes.length === 0"
                    class="scope-empty">None</span>
                </div>
              </div>
            </div>

            <div class="token-row__actions">
              <code class="token-row__id">#{{ token.token.id }}</code>
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
        </div>

        <div
          v-else
          class="empty-state text-center py-8">
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
        </div>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import http from "@/http";
import { useAlertsStore } from "@/stores/alerts";
import { useRepositoryStore } from "@/stores/repositories";
import { sessionStore } from "@/stores/session";
import { siteStore } from "@/stores/site";
import type { ScopeDescription } from "@/types/base";
import { type RawAuthTokenFullResponse } from "@/types/user/token";
import { ref } from "vue";

const session = sessionStore();
const user = session.user;
const repositories = useRepositoryStore();
const site = siteStore();
const authTokens = ref<Array<RawAuthTokenFullResponse>>([]);
const scopeDescriptions = ref<Map<string, ScopeDescription>>(new Map());
const loading = ref(true);
const error = ref<string | null>(null);
const alerts = useAlertsStore();

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString();
}

function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString();
}

function formatExpiration(iso?: string | null): string {
  if (!iso) {
    return "Never expires";
  }
  return `Expires ${formatDateTime(iso)}`;
}

function tokenStatus(token: RawAuthTokenFullResponse): {
  label: string;
  color: "success" | "warning" | "error";
  variant: "flat" | "outlined" | "tonal";
} {
  if (!token.token.active) {
    return { label: "Revoked", color: "error", variant: "outlined" };
  }
  if (token.token.expires_at && new Date(token.token.expires_at).getTime() <= Date.now()) {
    return { label: "Expired", color: "warning", variant: "tonal" };
  }
  return { label: "Active", color: "success", variant: "flat" };
}

function roleScopeName(scope: string): string {
  return scopeDescriptions.value.get(scope)?.name ?? scope;
}

function repositoryName(repositoryId: string): string {
  return repositories.getRepositoryFromCache(repositoryId)?.name ?? repositoryId;
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
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    const [response, scopes] = await Promise.all([
      http.get<Array<RawAuthTokenFullResponse>>("/api/user/token/list"),
      site.getScopes(),
      repositories.getRepositories(false),
    ]);
    scopeDescriptions.value = new Map(scopes.map((scope) => [String(scope.key), scope]));
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
.token-list {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 8px;
  overflow: hidden;
}

.token-row {
  display: grid;
  gap: 1rem;
  grid-template-columns: minmax(12rem, 1.2fr) minmax(16rem, 2fr) auto;
  padding: 1rem;
}

.token-row + .token-row {
  border-top: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
}

.token-row__main,
.token-row__scopes,
.token-row__actions {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.token-row__identity,
.token-scope-group {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.token-row__name {
  font-weight: 600;
}

.token-row__description,
.token-row__meta,
.token-scope-group__label,
.scope-empty,
.token-row__id {
  font-size: 0.9rem;
  color: var(--nr-text-secondary, rgba(0, 0, 0, 0.6));
}

.token-row__meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.token-scope-group__values {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.token-scope-group__values--stacked {
  flex-direction: column;
  align-items: flex-start;
}

.scope-pill,
.repo-scope {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 6px;
  padding: 0.25rem 0.5rem;
}

.repo-scope {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.repo-scope__name {
  font-weight: 600;
}

.repo-scope__actions,
.token-row__id {
  font-family: var(--nr-font-family-mono, "Roboto Mono", monospace);
}

.token-row__actions {
  align-items: flex-end;
  justify-content: space-between;
}

.empty-state {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  border-radius: 8px;
}

@media (max-width: 768px) {
  .token-row {
    grid-template-columns: 1fr;
  }

  .token-row__actions {
    align-items: flex-start;
  }

  .repo-scope {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
