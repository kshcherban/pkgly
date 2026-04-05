<template>
  <v-card
    class="auth-config"
    data-testid="auth-config-card">
    <v-card-text class="auth-config__content">
      <SwitchInput
        v-model="enabled"
        id="repository-auth-toggle"
        :disabled="isSaving">
        Require authentication for repository access
        <template #comment>
          Clients must authenticate using a Pkgly user/password or token before accessing this repository.
        </template>
      </SwitchInput>

      <v-alert
        v-if="!isCreate && alertState"
        density="comfortable"
        :type="alertState.type"
        variant="tonal"
        class="auth-config__status">
        {{ alertState.message }}
      </v-alert>
    </v-card-text>
  </v-card>
</template>

<script setup lang="ts">
import http from "@/http";
import SwitchInput from "@/components/form/SwitchInput.vue";
import { computed, nextTick, onMounted, ref, watch } from "vue";

const props = defineProps<{
  repository?: string;
}>();

const model = defineModel<{ enabled: boolean }>({
  default: { enabled: true },
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

type AlertState =
  | { type: "error"; message: string }
  | { type: "info"; message: string }
  | { type: "success"; message: string };

const alertState = computed<AlertState | null>(() => {
  if (!hasLoaded.value && !isSaving.value) {
    return null;
  }
  if (error.value) {
    return { type: "error" as const, message: `Failed to save: ${error.value}` };
  }
  if (isSaving.value) {
    return { type: "info" as const, message: "Saving…" };
  }
  if (hasUserInteracted.value) {
    return { type: "success" as const, message: "Authentication settings saved." };
  }
  return null;
});

onMounted(load);

watch(
  () => enabled.value,
  async (enabled) => {
    if (!props.repository || !hasLoaded.value || isSyncingRemote.value) {
      return;
    }
    error.value = null;
    hasUserInteracted.value = true;
    isSaving.value = true;
    try {
      await http.put(`/api/repository/${props.repository}/config/auth`, {
        enabled,
      });
    } catch (err: any) {
      console.error(err);
      error.value =
        err?.response?.data?.message ?? err?.message ?? "Unknown error";
    } finally {
      isSaving.value = false;
    }
  },
);

async function load() {
  if (!props.repository) {
    hasLoaded.value = true;
    return;
  }
  isSyncingRemote.value = true;
  try {
    const response = await http.get(
      `/api/repository/${props.repository}/config/auth`,
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
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.auth-config__content {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.auth-config__status {
  margin-top: 0.5rem;
}
</style>
