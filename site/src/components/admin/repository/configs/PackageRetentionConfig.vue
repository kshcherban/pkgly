<!--
ABOUTME: Edits package retention settings for a repository.
ABOUTME: Keeps changes in a draft until the administrator saves or cancels.
-->
<template>
  <v-card
    class="package-retention-config"
    data-testid="package-retention-config-card">
    <v-card-text class="package-retention-config__content">
      <SwitchInput
        v-model="draft.enabled"
        id="package-retention-enabled"
        :disabled="isSaving">
        Enabled
      </SwitchInput>

      <v-row dense>
        <v-col cols="12" md="6">
          <v-text-field
            id="package-retention-max-age"
            v-model="draft.maxAgeDays"
            type="number"
            label="Max age days"
            min="1"
            step="1"
            variant="outlined"
            density="comfortable"
            :error-messages="maxAgeDaysError"
            :disabled="isSaving"
            hide-details="auto" />
        </v-col>
        <v-col cols="12" md="6">
          <v-text-field
            id="package-retention-keep-latest"
            v-model="draft.keepLatestPerPackage"
            type="number"
            label="Keep latest per package"
            min="0"
            step="1"
            variant="outlined"
            density="comfortable"
            :error-messages="keepLatestPerPackageError"
            :disabled="isSaving"
            hide-details="auto" />
        </v-col>
      </v-row>

      <v-alert
        v-if="!isCreate && alertState"
        density="comfortable"
        :type="alertState.type"
        variant="tonal"
        class="package-retention-config__status">
        {{ alertState.message }}
      </v-alert>
    </v-card-text>
    <template v-if="!isCreate">
      <v-divider />
      <v-card-actions class="package-retention-config__actions">
        <v-btn
          variant="text"
          class="text-none"
          data-testid="package-retention-cancel"
          :disabled="isSaving || !hasChanges"
          @click="cancel">
          Cancel
        </v-btn>
        <SubmitButton
          :block="false"
          data-testid="package-retention-save"
          prepend-icon="mdi-content-save"
          :loading="isSaving"
          :disabled="!canSave"
          @click="save">
          Save
        </SubmitButton>
      </v-card-actions>
    </template>
  </v-card>
</template>

<script setup lang="ts">
import http from "@/http";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import { computed, onMounted, ref, watch } from "vue";

interface PackageRetentionConfig {
  enabled: boolean;
  max_age_days: number;
  keep_latest_per_package: number;
}

interface PackageRetentionDraft {
  enabled: boolean;
  maxAgeDays: string;
  keepLatestPerPackage: string;
}

const props = defineProps<{
  repository?: string;
}>();

const model = defineModel<PackageRetentionConfig>({
  default: {
    enabled: false,
    max_age_days: 30,
    keep_latest_per_package: 1,
  },
});

const isCreate = computed(() => !props.repository);
const isSaving = ref(false);
const error = ref<string | null>(null);
const hasLoaded = ref(false);
const hasUserInteracted = ref(false);
const savedConfig = ref<PackageRetentionConfig>(cloneConfig(model.value));
const draft = ref<PackageRetentionDraft>(draftFromConfig(model.value));

type AlertState =
  | { type: "error"; message: string }
  | { type: "info"; message: string }
  | { type: "success"; message: string };

const alertState = computed<AlertState | null>(() => {
  if (!hasLoaded.value && !isSaving.value) {
    return null;
  }
  if (error.value) {
    return { type: "error", message: `Failed to save: ${error.value}` };
  }
  if (isSaving.value) {
    return { type: "info", message: "Saving..." };
  }
  if (hasUserInteracted.value) {
    return { type: "success", message: "Retention settings saved." };
  }
  return null;
});

const maxAgeDaysError = computed(() => integerError(draft.value.maxAgeDays, 1));
const keepLatestPerPackageError = computed(() =>
  integerError(draft.value.keepLatestPerPackage, 0),
);
const isDraftValid = computed(
  () => !maxAgeDaysError.value && !keepLatestPerPackageError.value,
);
const parsedDraft = computed<PackageRetentionConfig | null>(() => {
  if (!isDraftValid.value) {
    return null;
  }
  return {
    enabled: draft.value.enabled,
    max_age_days: Number(draft.value.maxAgeDays),
    keep_latest_per_package: Number(draft.value.keepLatestPerPackage),
  };
});
const hasChanges = computed(() => !draftsEqual(draft.value, draftFromConfig(savedConfig.value)));
const canSave = computed(
  () => hasLoaded.value && hasChanges.value && isDraftValid.value && !isSaving.value,
);

watch(
  draft,
  () => {
    if (!isCreate.value || !isDraftValid.value) {
      return;
    }
    const nextConfig = parsedDraft.value;
    if (nextConfig) {
      model.value = nextConfig;
    }
  },
  { deep: true },
);

onMounted(load);

async function load() {
  if (!props.repository) {
    hasLoaded.value = true;
    savedConfig.value = cloneConfig(model.value);
    draft.value = draftFromConfig(model.value);
    return;
  }
  try {
    const response = await http.get(
      `/api/repository/${props.repository}/config/package_retention`,
      {
        params: { default: true },
      },
    );
    if (response?.data) {
      setCurrentConfig(response.data);
    }
  } catch (err) {
    console.error(err);
    error.value = "Failed to load configuration";
  } finally {
    hasLoaded.value = true;
  }
}

async function save() {
  if (!props.repository || !canSave.value) {
    return;
  }
  const nextConfig = parsedDraft.value;
  if (!nextConfig) {
    return;
  }

  error.value = null;
  isSaving.value = true;
  try {
    await http.put(`/api/repository/${props.repository}/config/package_retention`, nextConfig);
    setCurrentConfig(nextConfig);
    hasUserInteracted.value = true;
  } catch (err: any) {
    console.error(err);
    error.value = err?.response?.data?.message ?? err?.message ?? "Unknown error";
  } finally {
    isSaving.value = false;
  }
}

function cancel() {
  draft.value = draftFromConfig(savedConfig.value);
  error.value = null;
}

function setCurrentConfig(config: PackageRetentionConfig) {
  const nextConfig = cloneConfig(config);
  model.value = nextConfig;
  savedConfig.value = nextConfig;
  draft.value = draftFromConfig(nextConfig);
}

function cloneConfig(config: PackageRetentionConfig): PackageRetentionConfig {
  return { ...config };
}

function draftFromConfig(config: PackageRetentionConfig): PackageRetentionDraft {
  return {
    enabled: config.enabled,
    maxAgeDays: String(config.max_age_days),
    keepLatestPerPackage: String(config.keep_latest_per_package),
  };
}

function draftsEqual(left: PackageRetentionDraft, right: PackageRetentionDraft): boolean {
  return (
    left.enabled === right.enabled &&
    left.maxAgeDays === right.maxAgeDays &&
    left.keepLatestPerPackage === right.keepLatestPerPackage
  );
}

function integerError(value: string, minimum: number): string {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) {
    return `Enter a whole number greater than or equal to ${minimum}.`;
  }
  const parsed = Number(trimmed);
  if (!Number.isSafeInteger(parsed) || parsed < minimum) {
    return `Enter a whole number greater than or equal to ${minimum}.`;
  }
  return "";
}
</script>

<style scoped lang="scss">
.package-retention-config__content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.package-retention-config__status {
  margin-top: 0.25rem;
}

.package-retention-config__actions {
  justify-content: flex-end;
  gap: 0.75rem;
}
</style>
