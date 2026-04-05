<template>
  <main>
    <FloatingErrorBanner
      :visible="errorBanner.visible"
      :title="errorBanner.title"
      :message="errorBanner.message"
      @close="resetError" />
    <h1>Storage Create</h1>
    <form @submit.prevent="createStorage()">
      <TwoByFormBox>
        <TextInput
          id="storageName"
          v-model="input.name"
          autocomplete="none"
          required
          placeholder="Storage Name"
          >Storage Name</TextInput
        >
        <DropDown
          id="storageType"
          v-model="input.storageType"
          :options="storageOptions"
          required
          class="form-field--medium"
          >Storage Type</DropDown
        >
      </TwoByFormBox>
      <div
        v-if="storageConfig"
        class="storageConfig">
        <h2>{{ storageConfig.title }}</h2>
        <component
          :is="storageConfig.component"
          v-model="input.storageConfigValue"></component>
      </div>
      <SubmitButton
        v-if="storageConfig"
        class="primary-action"
        :block="false"
        prepend-icon="mdi-plus">
        Create
      </SubmitButton>
    </form>
  </main>
</template>

<script lang="ts" setup>
import DropDown from "@/components/form/dropdown/DropDown.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import TwoByFormBox from "@/components/form/TwoByFormBox.vue";
import FloatingErrorBanner from "@/components/ui/FloatingErrorBanner.vue";
import { getStorageType, storageTypes } from "@/components/nr/storage/storageTypes";
import type {
  S3StorageSettings,
  StorageTypeConfig,
} from "@/components/nr/storage/storageTypes";
import http from "@/http";
import router from "@/router";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref, watch } from "vue";
import { isAxiosError } from "axios";
const input = ref({
  name: "",
  storageType: "",
  storageConfigValue: storageTypes[0]!.defaultSettings(),
});
const storageOptions = ref(storageTypes);
const selectedStorageType = computed(() => getStorageType(input.value.storageType));
const storageConfig = computed(() => selectedStorageType.value);

const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});
const alerts = useAlertsStore();

const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};

watch(
  () => input.value.storageType,
  () => {
    resetError();
    const current = selectedStorageType.value;
    input.value.storageConfigValue = current
      ? current.defaultSettings()
      : storageTypes[0]!.defaultSettings(); // Default to Local storage config if none is selected
  },
  { immediate: true },
);

async function createStorage() {
  const selected = selectedStorageType.value;
  if (!selected) {
    const message = "Select a storage backend before submitting.";
    errorBanner.value = {
      visible: true,
      title: "Storage type required",
      message,
    };
    return;
  }
  const data = {
    name: input.value.name,
    config: {
      type: selected.configType,
      settings: input.value.storageConfigValue,
    },
  };

  const validationError = validateSettings(selected.value, data.config.settings);
  if (validationError) {
    errorBanner.value = {
      visible: true,
      title: "Invalid configuration",
      message: validationError,
    };
    return;
  }

  resetError();
  try {
    const response = await http.post(`/api/storage/new/${selected.value}`, data);
    alerts.success("Storage created", "The storage has been created.");
    router.push({
      name: "ViewStorage",
      params: { id: response.data.id },
    });
  } catch (err) {
    const resolved = resolveStorageError(err);
    errorBanner.value.visible = true;
    errorBanner.value.title = resolved.title;
    errorBanner.value.message = resolved.message;
    console.error(resolved.debugMessage);
  }
}

function validateSettings(selectedType: string, settings: StorageTypeConfig["settings"]): string | null {
  if (selectedType.toLowerCase() !== "s3") {
    return null;
  }
  const s3 = settings as S3StorageSettings;
  const access = s3.credentials?.access_key?.trim();
  const secret = s3.credentials?.secret_key?.trim();
  const role = s3.credentials?.role_arn?.trim();
  const hasAccess = Boolean(access);
  const hasSecret = Boolean(secret);
  if (hasAccess !== hasSecret) {
    return "Provide both access and secret keys, or leave both blank.";
  }
  if (hasAccess && role) {
    return "Static access keys and IAM role are mutually exclusive. Choose one authentication method.";
  }
  return null;
}

function resolveStorageError(error: unknown): {
  title: string;
  message: string;
  debugMessage: string;
} {
  const fallback = {
    title: "Unable to create storage",
    message: "An unexpected error occurred. Please try again.",
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

    if (status === 409) {
      return {
        title: "Storage name already exists",
        message:
          payloadMessage ??
          "A storage with the same name already exists. Choose a different storage name.",
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
form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  width: 100%;
  max-width: 720px;
  padding: 1rem 0;
}
.storageConfig {
  padding: 1rem;
  border: 1px solid var(--nr-border-color);
  border-radius: 0.5rem;
}
@media screen and (max-width: 1200px) {
  form {
    width: 100%;
  }
}
main {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  align-items: flex-start;
}

:deep(.primary-action) {
  align-self: flex-start;
  width: auto;
  min-width: 160px;
}

:deep(.form-field--medium) {
  max-width: 320px;
  width: 100%;
}
</style>
