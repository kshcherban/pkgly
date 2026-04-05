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
            <v-col cols="12" md="6">
              <TextInput
                id="tokenExpiration"
                v-model="newToken.tokenExpiration"
                disabled
                placeholder="Not implemented">
                Expiration
              </TextInput>
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
        <CopyCode
          data-testid="token-output"
          :code="newResponseTokenResponse.token" />
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
import CopyCode from "@/components/core/code/CopyCode.vue";
import ScopesSelector from "@/components/form/ScopesSelector.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import RepositoryToActionList from "@/components/nr/repository/RepositoryToActionList.vue";
import http from "@/http";
import type { RepositoryActions, ScopeDescription } from "@/types/base";
import type { RepositoryToActions } from "@/types/repository";
import { type NewAuthTokenResponse } from "@/types/user/token";
import { useAlertsStore } from "@/stores/alerts";
import { ref } from "vue";

const newToken = ref({
  tokenName: "",
  tokenDescription: "",
  tokenExpiration: "",
});
const isSubmitting = ref(false);
const newResponseTokenResponse = ref<NewAuthTokenResponse | undefined>(undefined);
const repositoryScopes = ref<Array<RepositoryToActions>>([]);
const scopes = ref<Array<ScopeDescription>>([]);
const alerts = useAlertsStore();

function resetForm() {
  newResponseTokenResponse.value = undefined;
  newToken.value = {
    tokenName: "",
    tokenDescription: "",
    tokenExpiration: "",
  };
  repositoryScopes.value = [];
  scopes.value = [];
}

async function createToken() {
  if (isSubmitting.value) {
    return;
  }
  isSubmitting.value = true;
  try {
    const repositoryScopesRequest = repositoryScopes.value.map((repositoryScope) => ({
      repository_string: repositoryScope.repositoryId,
      actions: repositoryScope.actions.asArray(),
    }));

    const scopesRequest = scopes.value.map((scope) => scope.key);

    const request = {
      name: newToken.value.tokenName,
      description: newToken.value.tokenDescription,
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
</style>
