<template>
  <form class="helm-config" @submit.prevent="save">
    <v-card
      class="config-card"
      data-testid="helm-config-container">
      <v-card-text>
        <v-row
          v-if="value"
          class="helm-config__grid"
          dense>
          <v-col
            cols="12"
            md="6"
            data-testid="helm-config-field">
            <DropDown
              v-model="value.mode"
              :options="modeOptions"
              required
              id="helm-mode">
              Repository Mode
            </DropDown>
          </v-col>

          <v-col
            cols="12"
            md="6"
            lg="4"
            data-testid="helm-config-field">
            <SwitchInput
              v-model="value.overwrite"
              id="helm-allow-overwrite">
              Allow Overwrite
            </SwitchInput>
          </v-col>

          <v-col
            cols="12"
            md="6"
            data-testid="helm-config-field">
            <TextInput
              v-model="publicBaseUrl"
              placeholder="https://charts.example.com/myrepo"
              id="helm-public-base-url">
              Public Base URL
            </TextInput>
          </v-col>

          <v-col
            cols="12"
            md="6"
            lg="3"
            data-testid="helm-config-field">
            <TextInput
              v-model="indexCacheTtl"
              inputmode="numeric"
              pattern="[0-9]*"
              placeholder="300"
              id="helm-index-ttl">
              Index Cache TTL (seconds)
            </TextInput>
          </v-col>

          <v-col
            cols="12"
            md="6"
            lg="3"
            data-testid="helm-config-field">
            <TextInput
              v-model="maxChartSize"
              inputmode="numeric"
              pattern="[0-9]*"
              placeholder="10485760"
              id="helm-max-chart-size">
              Max Chart Size (bytes)
            </TextInput>
          </v-col>

          <v-col
            cols="12"
            md="6"
            lg="3"
            data-testid="helm-config-field">
            <TextInput
              v-model="maxFileCount"
              inputmode="numeric"
              pattern="[0-9]*"
              placeholder="1024"
              id="helm-max-file-count">
              Max Files Per Chart
            </TextInput>
          </v-col>
        </v-row>

        <v-alert
          border="start"
          variant="tonal"
          color="info"
          class="mt-4"
          density="compact">
          Choose HTTP mode for classic index.yaml and tarball downloads, or OCI mode to expose Helm
          charts via the registry-compatible API.
        </v-alert>
      </v-card-text>

      <v-divider />

      <v-card-actions class="justify-start px-0">
        <SubmitButton
          v-if="!isCreate"
          data-testid="helm-config-save"
          :loading="isSaving"
          :disabled="isSaving"
          :block="false"
          prepend-icon="mdi-content-save">
          <span v-if="isSaving">Saving…</span>
          <span v-else>Save</span>
        </SubmitButton>
      </v-card-actions>
    </v-card>
  </form>
</template>

<script setup lang="ts">
import { computed, defineProps, onMounted, ref } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import http from "@/http";
import {
  defaultHelmConfig,
  type HelmRepositoryConfig,
  helmModeOptions,
} from "./helm";

const props = defineProps({
  settingName: String,
  repository: {
    type: String,
    required: false,
  },
});

const value = defineModel<HelmRepositoryConfig>({
  default: defaultHelmConfig(),
});

const isCreate = computed(() => !props.repository);
const isSaving = ref(false);

const modeOptions = helmModeOptions;

const publicBaseUrl = computed({
  get: () => value.value?.public_base_url ?? "",
  set: (input: string) => {
    if (!value.value) return;
    const trimmed = input.trim();
    value.value.public_base_url = trimmed === "" ? undefined : trimmed;
  },
});

function parseOptionalNumber(input: string): number | undefined {
  const trimmed = input.trim();
  if (trimmed === "") {
    return undefined;
  }
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : undefined;
}

const indexCacheTtl = computed({
  get: () => value.value?.index_cache_ttl?.toString() ?? "",
  set: (input: string) => {
    if (!value.value) return;
    value.value.index_cache_ttl = parseOptionalNumber(input);
  },
});

const maxChartSize = computed({
  get: () => value.value?.max_chart_size?.toString() ?? "",
  set: (input: string) => {
    if (!value.value) return;
    value.value.max_chart_size = parseOptionalNumber(input);
  },
});

const maxFileCount = computed({
  get: () => value.value?.max_file_count?.toString() ?? "",
  set: (input: string) => {
    if (!value.value) return;
    value.value.max_file_count = parseOptionalNumber(input);
  },
});

async function load() {
  if (!props.repository) {
    value.value = defaultHelmConfig();
    return;
  }
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/helm`);
    value.value = response.data;
  } catch (error) {
    console.error("Failed to load Helm config", error);
  }
}

async function save() {
  if (!props.repository || !value.value || isSaving.value) {
    return;
  }
  isSaving.value = true;
  try {
    await http.put(`/api/repository/${props.repository}/config/helm`, value.value);
    await load();
  } catch (error) {
    console.error("Failed to save Helm config", error);
  } finally {
    isSaving.value = false;
  }
}

onMounted(() => {
  if (!value.value) {
    value.value = defaultHelmConfig();
  }
  load();
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.helm-config {
  max-width: 100%;

  &__grid {
    row-gap: 1rem;
  }
}

.config-card {
  border: none;
  box-shadow: none;
  background-color: transparent;

  .v-card-text {
    padding-inline: 0;
  }

  .v-card-actions {
    padding-inline: 0;
  }
}
</style>
.config-card {
  border: none;
  box-shadow: none;
  background-color: transparent;

  .v-card-text {
    padding-inline: 0;
  }

  .v-card-actions {
    padding-inline: 0;
  }
}
