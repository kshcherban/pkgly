<template>
  <form
    class="maven-config"
    @submit.prevent="save">
    <v-card
      class="config-card"
      data-testid="maven-config-card">
      <v-card-text>
        <v-row
          v-if="isCreate"
          dense>
          <v-col cols="12" md="6">
            <DropDown
              v-model="selectedType"
              :options="mavenTypes"
              required
              id="maven-type">
              Repository Type
            </DropDown>
          </v-col>
        </v-row>

        <v-row
          v-else
          dense>
          <v-col cols="12" md="6">
            <TextInput
              v-model="selectedType"
              disabled
              id="maven-type-readonly">
              Repository Type
            </TextInput>
          </v-col>
        </v-row>

        <v-expand-transition>
          <div v-if="isProxy" class="mt-4">
            <MavenProxyConfig v-model="proxyConfig" />
            <ProxyCacheNotice class="mt-4" />
          </div>
        </v-expand-transition>
      </v-card-text>

      <v-divider v-if="!isCreate" />

      <v-card-actions
        v-if="!isCreate"
        class="justify-start">
        <SubmitButton
          :block="false"
          :loading="isSaving"
          :disabled="isSaving"
          prepend-icon="mdi-content-save">
          <span v-if="isSaving">Saving…</span>
          <span v-else>Save</span>
        </SubmitButton>
      </v-card-actions>
    </v-card>
  </form>
</template>

<script setup lang="ts">
import { computed, defineProps, onMounted, ref, watch } from "vue";
import { useAlertsStore } from "@/stores/alerts";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import MavenProxyConfig from "./MavenProxyConfig.vue";
import http from "@/http";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import { defaultProxy, type MavenConfigType, type MavenProxyConfigType } from "./maven";

const mavenTypes = [
  { value: "Hosted", label: "Hosted" },
  { value: "Proxy", label: "Proxy" },
];

const props = defineProps({
  settingName: String,
  repository: {
    type: String,
    required: false,
  },
});

const value = defineModel<MavenConfigType>({
  default: { type: "Hosted" },
});

const selectedType = ref<string>(value.value?.type ?? "Hosted");
const isCreate = computed(() => !props.repository);
const isSaving = ref(false);
const isLoading = ref(false);
const alerts = useAlertsStore();

const isProxy = computed(() => selectedType.value === "Proxy");

const proxyConfig = computed<MavenProxyConfigType>({
  get: () => {
    if (value.value?.type !== "Proxy" || !value.value.config) {
      return defaultProxy();
    }
    return value.value.config;
  },
  set: (config) => {
    if (value.value?.type === "Proxy") {
      value.value = { type: "Proxy", config };
    }
  },
});

watch(selectedType, (type) => {
  // Don't update model while loading from server
  if (isLoading.value) {
    return;
  }
  if (type === "Proxy") {
    value.value = {
      type: "Proxy",
      config: proxyConfig.value ?? defaultProxy(),
    };
  } else {
    value.value = { type: "Hosted" };
  }
});

async function load() {
  if (!props.repository) {
    return;
  }
  isLoading.value = true;
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/maven`);
    const data = response.data as MavenConfigType | null;
    if (!data) {
      selectedType.value = "Hosted";
      value.value = { type: "Hosted" };
      return;
    }
    // Set selectedType first to avoid triggering the watch
    selectedType.value = data.type;
    value.value = data;
  } catch (error) {
    console.error("Failed to load Maven config", error);
    alerts.error("Failed to load Maven configuration", "Check the server logs for details.");
  } finally {
    isLoading.value = false;
  }
}

async function save() {
  if (isCreate.value || !props.repository || !value.value || isSaving.value) {
    return;
  }
  isSaving.value = true;
  try {
    await http.put(`/api/repository/${props.repository}/config/maven`, value.value);
    alerts.success("Maven configuration saved", "Settings updated successfully.");
    await load();
  } catch (error) {
    console.error("Failed to save Maven config", error);
    alerts.error("Failed to save", "Unable to persist Maven configuration.");
  } finally {
    isSaving.value = false;
  }
}

onMounted(load);
</script>

<style scoped lang="scss">
.maven-config {
  max-width: 100%;
}

.config-card {
  border: none;
  box-shadow: none;
  background-color: transparent;

  .v-card-text,
  .v-card-actions {
    padding-inline: 0;
  }
}
</style>
