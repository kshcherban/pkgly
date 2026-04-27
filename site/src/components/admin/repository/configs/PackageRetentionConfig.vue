<template>
  <v-card
    class="package-retention-config"
    data-testid="package-retention-config-card">
    <v-card-text class="package-retention-config__content">
      <SwitchInput
        v-model="enabled"
        id="package-retention-enabled"
        :disabled="isSaving">
        Enabled
      </SwitchInput>

      <v-row dense>
        <v-col cols="12" md="6">
          <v-text-field
            id="package-retention-max-age"
            v-model.number="maxAgeDays"
            type="number"
            label="Max age days"
            min="1"
            step="1"
            variant="outlined"
            density="comfortable"
            :disabled="isSaving"
            hide-details="auto" />
        </v-col>
        <v-col cols="12" md="6">
          <v-text-field
            id="package-retention-keep-latest"
            v-model.number="keepLatestPerPackage"
            type="number"
            label="Keep latest per package"
            min="0"
            step="1"
            variant="outlined"
            density="comfortable"
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
  </v-card>
</template>

<script setup lang="ts">
import http from "@/http";
import SwitchInput from "@/components/form/SwitchInput.vue";
import { computed, nextTick, onMounted, ref, watch } from "vue";

interface PackageRetentionConfig {
  enabled: boolean;
  max_age_days: number;
  keep_latest_per_package: number;
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
const isSyncingRemote = ref(false);

const enabled = computed({
  get: () => model.value.enabled,
  set: (value: boolean) => {
    model.value = { ...model.value, enabled: value };
  },
});

const maxAgeDays = computed({
  get: () => model.value.max_age_days,
  set: (value: number) => {
    model.value = {
      ...model.value,
      max_age_days: normalizeInteger(value, 1),
    };
  },
});

const keepLatestPerPackage = computed({
  get: () => model.value.keep_latest_per_package,
  set: (value: number) => {
    model.value = {
      ...model.value,
      keep_latest_per_package: normalizeInteger(value, 0),
    };
  },
});

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

onMounted(load);

watch(
  () => model.value,
  async () => {
    if (!props.repository || !hasLoaded.value || isSyncingRemote.value) {
      return;
    }
    error.value = null;
    hasUserInteracted.value = true;
    isSaving.value = true;
    try {
      await http.put(`/api/repository/${props.repository}/config/package_retention`, model.value);
    } catch (err: any) {
      console.error(err);
      error.value = err?.response?.data?.message ?? err?.message ?? "Unknown error";
    } finally {
      isSaving.value = false;
    }
  },
  { deep: true },
);

async function load() {
  if (!props.repository) {
    hasLoaded.value = true;
    return;
  }
  isSyncingRemote.value = true;
  try {
    const response = await http.get(
      `/api/repository/${props.repository}/config/package_retention`,
      {
        params: { default: true },
      },
    );
    if (response?.data) {
      model.value = response.data;
    }
  } catch (err) {
    console.error(err);
    error.value = "Failed to load configuration";
  } finally {
    hasLoaded.value = true;
    await nextTick();
    isSyncingRemote.value = false;
  }
}

function normalizeInteger(value: number, minimum: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return minimum;
  }
  return Math.max(minimum, Math.floor(parsed));
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
</style>
