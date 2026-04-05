<template>
  <v-container class="py-6">
    <v-alert
      v-if="errorBanner.visible"
      type="error"
      variant="tonal"
      class="mb-4"
      closable
      @click:close="resetError">
      <div class="text-subtitle-1 font-weight-medium mb-1">{{ errorBanner.title }}</div>
      <div>{{ errorBanner.message }}</div>
    </v-alert>

    <v-card data-testid="user-create-card">
      <v-card-title class="d-flex align-center justify-space-between">
        <div>
          <div class="text-h6">Create User</div>
          <div class="text-body-2 text-medium-emphasis">
            Provision an account and optionally set an initial password.
          </div>
        </div>
      </v-card-title>

      <v-card-text>
        <v-form @submit.prevent="create">
          <v-row dense>
            <v-col cols="12" md="6">
              <TextInput
                v-model="user.name"
                :disabled="isSubmitting"
                required>
                Name
              </TextInput>
            </v-col>
            <v-col cols="12" md="6">
              <ValidatableTextBox
                id="email"
                type="email"
                :validations="EMAIL_VALIDATIONS"
                v-model="user.email"
                :disabled="isSubmitting"
                @validity="emailValid = $event">
                Email
              </ValidatableTextBox>
            </v-col>
            <v-col cols="12" md="6">
              <ValidatableTextBox
                id="username"
                :validations="USERNAME_VALIDATIONS"
                :deniedKeys="URL_SAFE_BAD_CHARS"
                v-model="user.username"
                :disabled="isSubmitting"
                @validity="usernameValid = $event">
                Username
              </ValidatableTextBox>
            </v-col>
          </v-row>

          <v-divider class="my-4" />

          <SwitchInput
            id="setPassword"
            v-model="setPassword"
            :disabled="isSubmitting">
            Set Password
            <template #comment>
              Disable to send the user an invite email instead.
            </template>
          </SwitchInput>

          <v-expand-transition>
            <div v-if="setPassword" class="mt-2">
              <NewPasswordInput
                id="password"
                :passwordRules="passwordRules"
                v-model="password"
                :disabled="isSubmitting" />
            </div>
          </v-expand-transition>

          <div class="d-flex justify-end mt-6">
            <SubmitButton
              :block="false"
              :disabled="!formIsValid || isSubmitting"
              :loading="isSubmitting"
              prepend-icon="mdi-account-plus">
              <span v-if="isSubmitting">Creating…</span>
              <span v-else>Create User</span>
            </SubmitButton>
          </div>
        </v-form>
      </v-card-text>
    </v-card>
  </v-container>
</template>
<script lang="ts" setup>
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import NewPasswordInput from "@/components/form/text/NewPasswordInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import ValidatableTextBox from "@/components/form/text/ValidatableTextBox.vue";
import {
  EMAIL_VALIDATIONS,
  URL_SAFE_BAD_CHARS,
  USERNAME_VALIDATIONS,
} from "@/components/form/text/validations";
import http from "@/http";
import router from "@/router";
import { siteStore } from "@/stores/site";
import type { PasswordRules } from "@/types/base";
import { isAxiosError } from "axios";
import { computed, watch, type Ref, ref } from "vue";
const user = ref({
  name: "",
  email: "",
  username: "",
});
const site = siteStore();
if (!site.siteInfo) {
  site.getInfo();
}
const passwordRules = computed(() => site.getPasswordRulesOrDefault());

const setPassword = ref(false);

const password: Ref<string | undefined> = ref(undefined);

const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});

const isSubmitting = ref(false);
const emailValid = ref(false);
const usernameValid = ref(false);

const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};

watch(
  () => [
    user.value.name,
    user.value.email,
    user.value.username,
  setPassword.value,
  password.value,
  emailValid.value,
  usernameValid.value,
  ],
  () => {
    if (errorBanner.value.visible) {
      resetError();
    }
  },
);

watch(setPassword, (enabled) => {
  if (!enabled) {
    password.value = undefined;
  }
});

const nameValid = computed(() => user.value.name.trim().length > 0);
const passwordValid = computed(() => !setPassword.value || !!password.value);
const formIsValid = computed(
  () => nameValid.value && emailValid.value && usernameValid.value && passwordValid.value,
);

async function create() {
  if (isSubmitting.value) {
    return;
  }

  if (!formIsValid.value) {
    errorBanner.value = {
      visible: true,
      title: "Review the form",
      message: "Please resolve validation errors before creating the user.",
    };
    return;
  }

  const requestBody = {
    name: user.value.name.trim(),
    email: user.value.email,
    username: user.value.username,
    password: password.value,
  };

  resetError();
  isSubmitting.value = true;
  try {
    await http.post("/api/user-management/create", requestBody);
    router.push("/admin/users");
  } catch (error) {
    const resolved = resolveUserCreateError(error);
    errorBanner.value = {
      visible: true,
      title: resolved.title,
      message: resolved.message,
    };
    console.error(resolved.debugMessage);
  } finally {
    isSubmitting.value = false;
  }
}

function resolveUserCreateError(error: unknown): {
  title: string;
  message: string;
  debugMessage: string;
} {
  const normalizeApiError = (
    data: unknown,
  ): { message?: string; details?: string | string[] } => {
    if (!data) {
      return {};
    }
    if (typeof data === "string") {
      return { message: data.trim() };
    }
    if (typeof data === "object") {
      const maybeMessage = (data as { message?: unknown }).message;
      const maybeDetails = (data as { details?: unknown }).details;
      return {
        message:
          typeof maybeMessage === "string" && maybeMessage.trim().length > 0
            ? maybeMessage.trim()
            : undefined,
        details:
          typeof maybeDetails === "string" || Array.isArray(maybeDetails)
            ? (maybeDetails as string | string[])
            : undefined,
      };
    }
    return {};
  };

  const fallback = {
    title: "Unable to create user",
    message: "An unexpected error occurred. Please review the form and try again.",
    debugMessage: typeof error === "string" ? error : JSON.stringify(error),
  };

  if (isAxiosError(error)) {
    const status = error.response?.status;
    const data = error.response?.data;
    const api = normalizeApiError(data);
    const payloadMessage = api.message;

    if (status === 400) {
      return {
        title: "Invalid user details",
        message:
          payloadMessage ??
          "Please ensure name, email, and username are present and valid, then try again.",
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    if (status === 409) {
      const details = Array.isArray(api.details)
        ? api.details
        : api.details
          ? [api.details]
          : [];
      if (details.some((item) => item.toLowerCase().includes("username"))) {
        return {
          title: "Username already exists",
          message:
            payloadMessage ??
            "A user with this username already exists. Choose a different username.",
          debugMessage: JSON.stringify(error.toJSON?.() ?? error),
        };
      }
      if (details.some((item) => item.toLowerCase().includes("email"))) {
        return {
          title: "Email already exists",
          message:
            payloadMessage ??
            "A user with this email address already exists. Enter a different email.",
          debugMessage: JSON.stringify(error.toJSON?.() ?? error),
        };
      }
      return {
        title: "User already exists",
        message:
          payloadMessage ??
          "A user with the same credentials already exists. Adjust the username or email.",
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    if (payloadMessage) {
      return {
        title: fallback.title,
        message: payloadMessage,
        debugMessage: JSON.stringify(error.toJSON?.() ?? error),
      };
    }

    return {
      title: fallback.title,
      message: `Request failed${status ? ` with status ${status}` : ""}.`,
      debugMessage: JSON.stringify(error.toJSON?.() ?? error),
    };
  }

  if (error instanceof Error) {
    return {
      title: fallback.title,
      message: error.message,
      debugMessage: error.stack ?? error.message,
    };
  }

  return fallback;
}
</script>

<style scoped lang="scss">
@use "@/assets/styles/tokens.scss" as *;

main {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  max-width: 640px;
}

form {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

:deep(.primary-action) {
  width: auto;
  min-width: 160px;
  padding-inline: 1.5rem;
  align-self: flex-start;
}
</style>
