<template>
  <v-container
    class="login-settings py-6">
    <v-row justify="center">
      <v-col cols="12" md="8" lg="6">
        <v-card>
          <v-card-title class="text-h6">Login Settings</v-card-title>
          <v-card-text>
            <div
              v-if="loading"
              class="text-center py-8">
              <v-progress-circular 
                indeterminate 
                color="primary"
                size="48" />
              <div class="mt-4 text-medium-emphasis">Loading password rules…</div>
            </div>

            <div
              v-else-if="!user"
              class="text-body-2 text-medium-emphasis">
              You must be signed in to manage your login settings.
            </div>

            <form
              v-else
              data-testid="email-form"
              @submit.prevent="changeEmail">
              <v-alert
                v-if="emailFeedback"
                :type="emailFeedback.type"
                variant="tonal"
                border="start"
                density="comfortable"
                class="mb-4"
                closable
                @click:close="emailFeedback = null">
                <div class="text-subtitle-1 font-weight-medium mb-1">
                  {{ emailFeedback.title }}
                </div>
                <div v-if="emailFeedback.message">
                  {{ emailFeedback.message }}
                </div>
              </v-alert>

              <TextInput
                id="profileEmail"
                v-model="email"
                type="email"
                autocomplete="email">
                Email Address
              </TextInput>

              <div class="d-flex justify-end mt-6">
                <SubmitButton
                  :block="false"
                  :disabled="!canSubmitEmail || isEmailSubmitting"
                  :loading="isEmailSubmitting"
                  prepend-icon="mdi-email-edit-outline">
                  Update Email
                </SubmitButton>
              </div>
            </form>

            <v-divider
              v-if="user"
              class="my-6" />

            <div
              v-if="user && !passwordRules"
              class="text-body-2 text-medium-emphasis">
              Password changes are currently disabled by the administrator.
            </div>

            <form
              v-else-if="user"
              data-testid="password-form"
              @submit.prevent="changePassword">
              <input
                id="email"
                type="hidden"
                name="email"
                autocomplete="email"
                :value="user.email" />
              <input
                id="username"
                type="hidden"
                name="username"
                autocomplete="username"
                :value="user.username" />

              <v-alert
                v-if="passwordFeedback"
                :type="passwordFeedback.type"
                variant="tonal"
                border="start"
                density="comfortable"
                class="mb-4"
                closable
                @click:close="passwordFeedback = null">
                <div class="text-subtitle-1 font-weight-medium mb-1">
                  {{ passwordFeedback.title }}
                </div>
                <div v-if="passwordFeedback.message">
                  {{ passwordFeedback.message }}
                </div>
              </v-alert>

              <PasswordInput
                id="currentPassword"
                v-model="oldPassword"
                label="Current Password" />

              <NewPasswordInput
                id="newPassword"
                v-model="newPassword"
                :passwordRules="passwordRules">
                New Password
              </NewPasswordInput>

              <div class="d-flex justify-end mt-6">
                <SubmitButton
                  :block="false"
                  :disabled="!canSubmitPassword || isPasswordSubmitting"
                  :loading="isPasswordSubmitting"
                  prepend-icon="mdi-content-save">
                  Change Password
                </SubmitButton>
              </div>
            </form>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </v-container>
</template>
<script setup lang="ts">
import SubmitButton from "@/components/form/SubmitButton.vue";
import NewPasswordInput from "@/components/form/text/NewPasswordInput.vue";
import PasswordInput from "@/components/form/text/PasswordInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import http from "@/http";
import { sessionStore } from "@/stores/session";
import { siteStore } from "@/stores/site";
import type { UserResponseType } from "@/types/base";
import type { AxiosError } from "axios";
import { computed, onMounted, ref, watch } from "vue";
const site = siteStore();
const session = sessionStore();
const user = computed(() => session.user);
const email = ref("");
const oldPassword = ref("");
const newPassword = ref("");
const isEmailSubmitting = ref(false);
const isPasswordSubmitting = ref(false);
const loading = ref(false);
const emailFeedback = ref<{ type: "success" | "error"; title: string; message?: string } | null>(null);
const passwordFeedback = ref<{ type: "success" | "error"; title: string; message?: string } | null>(null);

watch(
  user,
  (nextUser) => {
    email.value = nextUser?.email ?? "";
  },
  { immediate: true },
);

onMounted(async () => {
  if (!site.siteInfo) {
    loading.value = true;
    try {
      await site.getInfo?.();
    } finally {
      loading.value = false;
    }
  }
});

const passwordRules = computed(() => site.siteInfo?.password_rules);

const canSubmitEmail = computed(() => {
  if (isEmailSubmitting.value || !user.value) {
    return false;
  }
  const trimmed = email.value.trim();
  if (!trimmed) {
    return false;
  }
  return trimmed !== user.value.email;
});

const canSubmitPassword = computed(() => {
  if (isPasswordSubmitting.value) {
    return false;
  }
  const current = oldPassword.value.trim().length > 0;
  const next = (newPassword.value ?? "").trim().length > 0;
  if (!current || !next) {
    return false;
  }
  return true;
});

async function changeEmail() {
  if (!user.value || !canSubmitEmail.value) {
    return;
  }

  emailFeedback.value = null;
  isEmailSubmitting.value = true;
  try {
    const response = await http.post<UserResponseType>("/api/user/change-email", {
      email: email.value.trim(),
    });
    session.user = response.data;
    email.value = response.data.email;
    emailFeedback.value = {
      type: "success",
      title: "Email updated",
      message: "Your email address was updated successfully.",
    };
  } catch (error) {
    console.error(error);
    let message = "Failed to update email.";
    if (isAxiosError(error)) {
      if (error.response?.status === 409) {
        message = "That email address is already in use.";
      } else {
        const responseText =
          typeof error.response?.data === "string" ? error.response.data : undefined;
        message = responseText ?? message;
      }
    }
    emailFeedback.value = {
      type: "error",
      title: "Email update failed",
      message,
    };
  } finally {
    isEmailSubmitting.value = false;
  }
}

async function changePassword() {
  if (!user.value) {
    return;
  }
  if (!canSubmitPassword.value) {
    return;
  }
  passwordFeedback.value = null;
  const request = {
    old_password: oldPassword.value,
    new_password: newPassword.value,
  };
  isPasswordSubmitting.value = true;
  try {
    await http.post("/api/user/change-password", request);
    oldPassword.value = "";
    newPassword.value = "";
    passwordFeedback.value = {
      type: "success",
      title: "Password updated",
      message: "Your password was changed successfully.",
    };
  } catch (error) {
    console.error(error);
    let message = "Failed to change password.";
    if (isAxiosError(error)) {
      const responseText = typeof error.response?.data === "string" ? error.response.data : undefined;
      message = responseText ?? message;
    }
    passwordFeedback.value = {
      type: "error",
      title: "Password update failed",
      message,
    };
  } finally {
    isPasswordSubmitting.value = false;
  }
}

function isAxiosError(error: unknown): error is AxiosError {
  return Boolean(error) && typeof error === "object" && "isAxiosError" in (error as any);
}
</script>

<style scoped lang="scss">
.login-settings {
  max-width: 960px;
}
</style>
