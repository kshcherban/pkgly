<template>
  <v-container
    v-if="user"
    class="admin-user-page pa-0">
    <FloatingErrorBanner
      :visible="errorBanner.visible"
      :title="errorBanner.title"
      :message="errorBanner.message"
      @close="resetError" />

    <v-card
      variant="flat"
      class="admin-user-page__card">
      <v-tabs
        v-model="currentTab"
        density="comfortable"
        class="admin-user-page__tabs"
        data-testid="admin-user-tabs">
        <v-tab
          value="main"
          data-testid="admin-user-tab">
          User
        </v-tab>
        <v-tab
          value="password"
          data-testid="admin-user-tab">
          Password
        </v-tab>
        <v-tab
          value="user-permissions"
          data-testid="admin-user-tab">
          User Permissions
        </v-tab>
        <v-tab
          value="repository-permissions"
          data-testid="admin-user-tab">
          Repository Permissions
        </v-tab>
      </v-tabs>

      <v-divider />

      <v-window
        v-model="currentTab"
        class="admin-user-page__window">
        <v-window-item value="main">
          <section class="admin-user-page__section">
            <header class="admin-user-page__status">
              <span
                class="admin-user-page__status-badge"
                :data-active="user.active">
                {{ user.active ? "Active" : "Inactive" }}
              </span>
              <div class="admin-user-page__status-actions">
                <v-btn
                  variant="outlined"
                  color="medium-emphasis"
                  class="text-none"
                  :disabled="statusUpdating"
                  @click="setActive(!user.active)">
                  {{ user.active ? "Deactivate" : "Reactivate" }}
                </v-btn>
                <v-btn
                  color="error"
                  variant="flat"
                  class="text-none"
                  prepend-icon="mdi-delete-outline"
                  :disabled="deletingUser || isCurrentUser"
                  @click="deleteUser">
                  Delete User
                </v-btn>
              </div>
            </header>

            <form
              class="admin-user-page__form"
              @submit.prevent="saveUserDetails">
              <TextInput
                id="name"
                v-model="changeUser.name"
                autocomplete="name">
                Name
              </TextInput>
              <ValidatableTextBox
                id="email"
                autocomplete="email"
                :validations="EMAIL_VALIDATIONS"
                :originalValue="user.email"
                v-model="changeUser.email">
                Email
              </ValidatableTextBox>
              <ValidatableTextBox
                id="username"
                :originalValue="user.username"
                :validations="USERNAME_VALIDATIONS"
                :deniedKeys="[' ']"
                autocomplete="username"
                v-model="changeUser.username">
                Username
              </ValidatableTextBox>
              <div class="admin-user-page__actions">
                <SubmitButton
                  :block="false"
                  :loading="savingUser"
                  :disabled="savingUser"
                  prepend-icon="mdi-content-save">
                  Save
                </SubmitButton>
              </div>
            </form>

            <dl class="admin-user-page__metadata">
              <KeyAndValue
                :label="'ID #'"
                :value="user.id.toLocaleString()" />
              <KeyAndValue
                :label="'Status'"
                :value="user.active ? 'Active' : 'Inactive'" />
              <KeyAndValue
                :label="'Created At'"
                :value="new Date(user.created_at).toLocaleString()" />
            </dl>
          </section>
        </v-window-item>

        <v-window-item value="password">
          <form
            id="setPassword"
            class="admin-user-page__password-form"
            data-testid="admin-user-password-form"
            @submit.prevent="changePassword">
            <input
              type="hidden"
              name="email"
              autocomplete="email"
              :value="user.email" />
            <input
              type="hidden"
              name="username"
              autocomplete="username"
              :value="user.username" />
            <NewPasswordInput
              id="password"
              v-model="newPassword"
              :passwordRules="passwordRules">
              Password
            </NewPasswordInput>
            <div class="admin-user-page__actions">
              <SubmitButton
                :block="false"
                :disabled="!newPassword"
                prepend-icon="mdi-content-save">
                Save
              </SubmitButton>
            </div>
          </form>
        </v-window-item>

        <v-window-item value="user-permissions">
          <section class="admin-user-page__section">
            <UserPermissions :user="user" />
          </section>
        </v-window-item>

        <v-window-item value="repository-permissions">
          <section class="admin-user-page__section">
            <RepositoryPermissions :user="user" />
          </section>
        </v-window-item>
      </v-window>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import KeyAndValue from "@/components/form/KeyAndValue.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import NewPasswordInput from "@/components/form/text/NewPasswordInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import { siteStore } from "@/stores/site";
import { sessionStore } from "@/stores/session";
import type { UserResponseType } from "@/types/base";
import FloatingErrorBanner from "@/components/ui/FloatingErrorBanner.vue";
import { computed, ref, type PropType, watch } from "vue";
import UserPermissions from "./UserPermissions.vue";
import RepositoryPermissions from "./RepositoryPermissions.vue";
import http from "@/http";
import { useAlertsStore } from "@/stores/alerts";
import ValidatableTextBox from "@/components/form/text/ValidatableTextBox.vue";
import { EMAIL_VALIDATIONS, USERNAME_VALIDATIONS } from "@/components/form/text/validations";
import { isAxiosError } from "axios";
const props = defineProps({
  user: {
    type: Object as PropType<UserResponseType>,
    required: true,
  },
});
const emit = defineEmits<{
  (e: "refresh"): void;
  (e: "deleted"): void;
}>();
const currentTab = ref("main");
const changeUser = ref({
  name: "",
  email: "",
  username: "",
});
const newPassword = ref<string | undefined>(undefined);

const site = siteStore();
if (!site.siteInfo) {
  site.getInfo();
}
const passwordRules = computed(() => site.getPasswordRulesOrDefault());
const session = sessionStore();
const isCurrentUser = computed(() => session.user?.id === props.user.id);
const statusUpdating = ref(false);
const deletingUser = ref(false);
const savingUser = ref(false);
const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});

const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};

const showError = (title: string, message: string) => {
  errorBanner.value.visible = true;
  errorBanner.value.title = title;
  errorBanner.value.message = message;
};

const alerts = useAlertsStore();

watch(
  () => props.user,
  (newUser) => {
    if (!newUser) {
      return;
    }
    changeUser.value = {
      name: newUser.name,
      email: newUser.email,
      username: newUser.username,
    };
    resetError();
  },
  { immediate: true },
);

watch(newPassword, () => {
  if (errorBanner.value.visible) {
    resetError();
  }
});

async function changePassword() {
  console.log("Changing Password");

  if (!newPassword.value) {
    alerts.error("Password required", "Enter and confirm a password before saving.");
    return;
  }

  console.log("Password is valid");

  try {
    await http.put(`/api/user-management/update/${props.user.id}/password`, {
      password: newPassword.value,
    });
    alerts.success("Password changed", "Password has been changed.");
    newPassword.value = undefined;
    console.log("Password Changed");
  } catch (error) {
    const resolved = resolveUserOperationError(
      error,
      "Unable to change password",
      "Review the password requirements and try again.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  }
}

async function saveUserDetails() {
  if (savingUser.value) {
    return;
  }
  resetError();
  savingUser.value = true;
  try {
    await http.put(`/api/user-management/update/${props.user.id}`, {
      name: changeUser.value.name,
      email: changeUser.value.email,
      username: changeUser.value.username,
    });
    alerts.success("User updated", "User profile details have been saved.");
    emit("refresh");
  } catch (error) {
    const resolved = resolveUserOperationError(
      error,
      "Unable to update user",
      "Review the provided details and try again.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    savingUser.value = false;
  }
}

async function setActive(active: boolean) {
  if (statusUpdating.value || props.user.active === active) {
    return;
  }
  statusUpdating.value = true;
  try {
    await http.put(`/api/user-management/update/${props.user.id}/status`, {
      active,
    });
    alerts.success(active ? "User reactivated" : "User deactivated");
    emit("refresh");
  } catch (error: any) {
    console.error(error);
    const resolved = resolveUserOperationError(
      error,
      "Unable to update status",
      "Failed to update user status.",
    );
    showError(resolved.title, resolved.message);
  } finally {
    statusUpdating.value = false;
  }
}

async function deleteUser() {
  if (deletingUser.value) {
    return;
  }
  if (
    !window.confirm(
      `Are you sure you want to delete user "${props.user.username}"? This action cannot be undone.`,
    )
  ) {
    return;
  }
  deletingUser.value = true;
  try {
    await http.delete(`/api/user-management/delete/${props.user.id}`);
    alerts.success("User deleted");
    emit("deleted");
  } catch (error: any) {
    console.error(error);
    const resolved = resolveUserOperationError(
      error,
      "Unable to delete user",
      "Failed to delete user.",
    );
    showError(resolved.title, resolved.message);
  } finally {
    deletingUser.value = false;
  }
}

function resolveUserOperationError(
  error: unknown,
  fallbackTitle: string,
  fallbackMessage: string,
): { title: string; message: string; debugMessage: string } {
  const fallback = {
    title: fallbackTitle,
    message: fallbackMessage,
    debugMessage: typeof error === "string" ? error : JSON.stringify(error),
  };

  if (isAxiosError(error)) {
    const status = error.response?.status;
    const data = error.response?.data;
    const payloadMessage =
      (typeof data === "string" && data.trim().length > 0 && data.trim()) ||
      (typeof data === "object" &&
        data !== null &&
        "message" in data &&
        typeof (data as { message?: unknown }).message === "string" &&
        (data as { message: string }).message.trim().length > 0
        ? (data as { message: string }).message.trim()
        : undefined);

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
@use "@/assets/styles/theme" as *;

.admin-user-page__card {
  border-radius: 16px;
  overflow: hidden;
}

.admin-user-page__tabs {
  padding-inline: 1rem;
}

.admin-user-page__window {
  padding: 1.5rem;
}

.admin-user-page__section {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.admin-user-page__status {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 1rem;
}

.admin-user-page__status-badge {
  padding: 0.35rem 0.75rem;
  border-radius: 999px;
  font-weight: 600;
  background-color: $primary-30;
  color: $text;

  &[data-active="true"] {
    background-color: $primary-70;
    color: $background;
  }

  &[data-active="false"] {
    background-color: $secondary-70;
    color: $text;
  }
}

.admin-user-page__status-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.admin-user-page__form {
  display: grid;
  gap: 1rem;
  max-width: 520px;
}

.admin-user-page__actions {
  margin-top: 0.5rem;
  display: flex;
  gap: 0.75rem;
}

.admin-user-page__actions :deep(.submit-button) {
  min-width: 160px;
}

.admin-user-page__metadata {
  display: grid;
  gap: 0.75rem;
  max-width: 360px;
}

.admin-user-page__password-form {
  display: grid;
  gap: 1rem;
  max-width: 520px;
}

@media screen and (max-width: 600px) {
  .admin-user-page__window {
    padding: 1rem;
  }

  .admin-user-page__form,
  .admin-user-page__password-form {
    max-width: 100%;
  }
}
</style>
