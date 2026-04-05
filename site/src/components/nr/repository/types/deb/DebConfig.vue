<template>
  <form class="deb-config" @submit.prevent="save">
    <DropDown
      v-model="selectedType"
      :options="typeOptions"
      :disabled="!isCreate"
      class="full-width"
      required
    >Repository Type</DropDown>

    <template v-if="isHosted">
      <v-combobox
        v-model="hosted.distributions"
        multiple
        chips
        clearable
        persistent-hint
        hide-details="auto"
        label="Distributions"
        hint="Suites available for clients (e.g. stable, testing)"
      />
      <v-combobox
        v-model="hosted.components"
        multiple
        chips
        clearable
        persistent-hint
        hide-details="auto"
        label="Components"
        hint="Logical sections like main, contrib, non-free"
      />
      <v-combobox
        v-model="hosted.architectures"
        multiple
        chips
        clearable
        persistent-hint
        hide-details="auto"
        label="Architectures"
        hint="Architectures accepted on upload (e.g. amd64, arm64, all)"
      />
    </template>

    <template v-else>
      <TextInput v-model="proxy.upstream_url" required placeholder="https://deb.debian.org/debian"
        >Upstream URL</TextInput
      >

      <DropDown
        v-model="selectedLayout"
        :options="layoutOptions"
        :disabled="!isCreate"
        class="full-width"
        required
      >Upstream Layout</DropDown>

      <template v-if="isDistsLayout">
        <v-combobox
          v-model="proxyDists.distributions"
          multiple
          chips
          clearable
          persistent-hint
          hide-details="auto"
          label="Distributions"
          hint="Suites mirrored from upstream (e.g. stable, bookworm)"
        />
        <v-combobox
          v-model="proxyDists.components"
          multiple
          chips
          clearable
          persistent-hint
          hide-details="auto"
          label="Components"
          hint="Components mirrored from upstream (e.g. main, contrib, non-free)"
        />
        <v-combobox
          v-model="proxyDists.architectures"
          multiple
          chips
          clearable
          persistent-hint
          hide-details="auto"
          label="Architectures"
          hint="Architectures mirrored from upstream (e.g. amd64, arm64, all)"
        />
      </template>

      <template v-else>
        <TextInput v-model="proxyFlat.distribution" required placeholder="./"
          >Distribution Path</TextInput
        >
        <v-combobox
          v-model="proxyFlat.architectures"
          multiple
          chips
          clearable
          persistent-hint
          hide-details="auto"
          label="Architectures (optional)"
          hint="Optional architecture filter; leave empty to accept all architectures present in Packages"
        />
      </template>

      <div class="deb-config__refresh-controls">
        <v-divider class="mt-2" />

        <SwitchInput
          v-model="refreshEnabledUi"
          id="deb-proxy-refresh-enabled"
          data-testid="deb-refresh-enabled"
        >Enable automatic mirror refresh</SwitchInput>

        <DropDown
          v-model="selectedRefreshType"
          :options="refreshTypeOptions"
          class="full-width"
          :disabled="!refreshEnabledUi"
          required
        >Refresh Schedule</DropDown>

        <template v-if="refreshEnabledUi && selectedRefreshType === 'interval_seconds'">
          <v-text-field
            v-model.number="refreshIntervalSeconds"
            type="number"
            min="1"
            step="1"
            variant="outlined"
            density="comfortable"
            label="Interval (seconds)"
            persistent-hint
            hide-details="auto"
            hint="Runs a refresh at a fixed interval"
          />
        </template>

        <template v-if="refreshEnabledUi && selectedRefreshType === 'cron'">
          <TextInput
            v-model="refreshCronExpression"
            placeholder="0 3 * * *"
            have-clear-button
          >Cron (UTC)</TextInput>
          <div class="hint">Accepts 5-field and 7-field cron formats.</div>
        </template>

        <v-alert
          v-if="refreshStatus"
          class="mt-2"
          :type="refreshStatus.due ? 'warning' : 'info'"
          variant="tonal"
          density="compact"
        >
          <div>
            <strong>Status:</strong>
            <span v-if="refreshStatus.in_progress">running</span>
            <span v-else>idle</span>
            <span v-if="refreshStatus.last_error"> (last error: {{ refreshStatus.last_error }})</span>
          </div>
          <div v-if="refreshStatus.next_run_at"><strong>Next run:</strong> {{ refreshStatus.next_run_at }}</div>
          <div v-else><strong>Next run:</strong> n/a</div>
        </v-alert>

        <v-alert
          v-if="manualRefreshNotice"
          class="mt-2"
          :type="manualRefreshNotice.type"
          variant="tonal"
          density="compact"
        >
          {{ manualRefreshNotice.message }}
        </v-alert>

        <SubmitButton
          v-if="!isCreate"
          data-testid="deb-refresh-mirror"
          type="button"
          :block="false"
          prepend-icon="mdi-refresh"
          :loading="isManualRefreshing"
          :disabled="isManualRefreshing || !!refreshStatus?.in_progress"
          @click="refreshMirror"
        >
          Refresh Mirror
        </SubmitButton>
      </div>

      <ProxyCacheNotice class="mt-2" />
    </template>

    <SubmitButton v-if="!isCreate" :block="false" prepend-icon="mdi-content-save">
      Save
    </SubmitButton>
  </form>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import http from "@/http";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import {
  defaultDebConfig,
  defaultDebHostedConfig,
  defaultDebProxyConfig,
  isDebProxyConfig,
  type DebHostedConfig,
  type DebProxyConfig,
  type DebRepositoryConfig,
  type DebProxyRefreshConfig,
  type DebProxyRefreshSchedule,
} from "./deb";

const props = defineProps({
  repository: {
    type: String,
    required: false,
  },
});

const value = defineModel<DebRepositoryConfig>({
  default: defaultDebConfig(),
});

const isCreate = computed(() => !props.repository);
const isHosted = computed(() => !isDebProxyConfig(value.value));
const isProxy = computed(() => isDebProxyConfig(value.value));

const typeOptions = [
  { value: "Hosted", label: "Hosted" },
  { value: "Proxy", label: "Proxy" },
];

const layoutOptions = [
  { value: "dists", label: "dists/ + pool/ (standard)" },
  { value: "flat", label: "Flat (Packages at root)" },
];

const refreshTypeOptions = [
  { value: "interval_seconds", label: "Interval (seconds)" },
  { value: "cron", label: "Cron (UTC)" },
];

const selectedType = ref<string>(isProxy.value ? "Proxy" : "Hosted");
const selectedLayout = ref<string>("dists");
const selectedRefreshType = ref<string>("interval_seconds");

type DebProxyRefreshStatusResponse = {
  in_progress: boolean;
  last_started_at: string | null;
  last_finished_at: string | null;
  last_success_at: string | null;
  last_error: string | null;
  last_downloaded_packages: number | null;
  last_downloaded_files: number | null;
  due: boolean;
  next_run_at: string | null;
};

const refreshStatus = ref<DebProxyRefreshStatusResponse | null>(null);

const manualRefreshNotice = ref<{ type: "success" | "info" | "error"; message: string } | null>(null);
const isManualRefreshing = ref(false);

const hosted = computed<DebHostedConfig>({
  get() {
    if (isDebProxyConfig(value.value)) {
      if (value.value.config.layout.type === "dists") {
        return value.value.config.layout.config;
      }
      return defaultDebHostedConfig();
    }
    return value.value ?? defaultDebHostedConfig();
  },
  set(next) {
    if (isDebProxyConfig(value.value)) {
      if (value.value.config.layout.type === "dists") {
        value.value.config.layout.config = next;
      }
      return;
    }
    value.value = next;
  },
});

const proxy = computed<DebProxyConfig>({
  get() {
    if (isDebProxyConfig(value.value)) {
      return value.value.config;
    }
    return defaultDebProxyConfig();
  },
  set(next) {
    if (isDebProxyConfig(value.value)) {
      value.value.config = next;
      return;
    }
    value.value = { type: "proxy", config: next };
  },
});

const isDistsLayout = computed(() => isDebProxyConfig(value.value) && value.value.config.layout.type === "dists");

const proxyDists = computed<DebHostedConfig>({
  get() {
    const current = proxy.value;
    if (current.layout.type !== "dists") {
      return defaultDebHostedConfig();
    }
    return current.layout.config;
  },
  set(next) {
    const current = proxy.value;
    proxy.value = {
      ...current,
      layout: { type: "dists", config: next },
    };
  },
});

const proxyFlat = computed<{ distribution: string; architectures: string[] }>({
  get() {
    const current = proxy.value;
    if (current.layout.type !== "flat") {
      return { distribution: "./", architectures: [] };
    }
    return current.layout.config;
  },
  set(next) {
    const current = proxy.value;
    proxy.value = {
      ...current,
      layout: { type: "flat", config: next },
    };
  },
});

const refresh = computed<DebProxyRefreshConfig>({
  get() {
    const current = proxy.value;
    return current.refresh ?? defaultDebProxyConfig().refresh!;
  },
  set(next) {
    const current = proxy.value;
    proxy.value = {
      ...current,
      refresh: next,
    };
  },
});

const refreshEnabledUi = ref(false);

const suppressRefreshEnabledAutosave = ref(true);
const isSavingRefreshEnabled = ref(false);
const isRevertingRefreshEnabledUi = ref(false);
let refreshEnabledSaveSequence = 0;

watch(
  refreshEnabledUi,
  (newValue, oldValue) => {
    if (!isDebProxyConfig(value.value)) {
      return;
    }

    refresh.value = {
      ...refresh.value,
      enabled: newValue,
    };

    if (isRevertingRefreshEnabledUi.value) {
      return;
    }

    if (suppressRefreshEnabledAutosave.value) {
      return;
    }
    if (!props.repository || isCreate.value || !isProxy.value) {
      return;
    }
    if (newValue === oldValue) {
      return;
    }

    const sequence = ++refreshEnabledSaveSequence;
    void (async () => {
      isSavingRefreshEnabled.value = true;
      try {
        await http.put(`/api/repository/${props.repository}/config/deb`, value.value);
      } catch (error) {
        if (sequence === refreshEnabledSaveSequence) {
          isRevertingRefreshEnabledUi.value = true;
          refreshEnabledUi.value = oldValue;
          isRevertingRefreshEnabledUi.value = false;
        }
      } finally {
        if (sequence === refreshEnabledSaveSequence) {
          isSavingRefreshEnabled.value = false;
          await loadRefreshStatus();
        }
      }
    })();
  },
  { flush: "sync" },
);

const refreshIntervalSeconds = computed<number>({
  get() {
    const schedule = refresh.value.schedule;
    if (schedule.type !== "interval_seconds") {
      return 3600;
    }
    return schedule.config.interval_seconds;
  },
  set(next) {
    const current = refresh.value;
    refresh.value = {
      ...current,
      schedule: { type: "interval_seconds", config: { interval_seconds: Math.max(1, next) } },
    };
  },
});

const refreshCronExpression = computed<string>({
  get() {
    const schedule = refresh.value.schedule;
    if (schedule.type !== "cron") {
      return "0 3 * * *";
    }
    return schedule.config.expression;
  },
  set(next) {
    const current = refresh.value;
    refresh.value = {
      ...current,
      schedule: { type: "cron", config: { expression: next } },
    };
  },
});

function normalize() {
  if (!value.value) {
    value.value = defaultDebConfig();
    refreshEnabledUi.value = false;
    return;
  }

  if (isDebProxyConfig(value.value)) {
    selectedType.value = "Proxy";
    const config = value.value.config ?? defaultDebProxyConfig();
    if (!config.upstream_url) {
      config.upstream_url = defaultDebProxyConfig().upstream_url;
    }
    if (!config.layout || (config.layout.type !== "dists" && config.layout.type !== "flat")) {
      config.layout = defaultDebProxyConfig().layout;
    }
    if (!config.refresh || typeof config.refresh !== "object") {
      config.refresh = defaultDebProxyConfig().refresh;
    }
    if (config.refresh) {
      if (typeof config.refresh.enabled !== "boolean") {
        config.refresh.enabled = false;
      }
      const schedule = (config.refresh.schedule ?? defaultDebProxyConfig().refresh!.schedule) as DebProxyRefreshSchedule;
      if (schedule.type !== "interval_seconds" && schedule.type !== "cron") {
        config.refresh.schedule = defaultDebProxyConfig().refresh!.schedule;
      } else {
        config.refresh.schedule = schedule;
      }
      selectedRefreshType.value = config.refresh.schedule.type;
    }
    if (config.layout.type === "dists") {
      const dists = config.layout.config ?? defaultDebHostedConfig();
      if (!Array.isArray(dists.distributions) || dists.distributions.length === 0) {
        dists.distributions = ["stable"];
      }
      if (!Array.isArray(dists.components) || dists.components.length === 0) {
        dists.components = ["main"];
      }
      if (!Array.isArray(dists.architectures) || dists.architectures.length === 0) {
        dists.architectures = ["amd64", "all"];
      }
      config.layout = { type: "dists", config: dists };
      selectedLayout.value = "dists";
    } else {
      const flat = config.layout.config ?? { distribution: "./", architectures: [] };
      if (!flat.distribution || typeof flat.distribution !== "string") {
        flat.distribution = "./";
      }
      if (!Array.isArray(flat.architectures)) {
        flat.architectures = [];
      }
      config.layout = { type: "flat", config: flat };
      selectedLayout.value = "flat";
    }
    value.value = { type: "proxy", config };
    refreshEnabledUi.value = config.refresh?.enabled ?? false;
    return;
  }

  selectedType.value = "Hosted";
  const hostedValue = value.value as DebHostedConfig;
  if (!Array.isArray(hostedValue.distributions) || hostedValue.distributions.length === 0) {
    hostedValue.distributions = ["stable"];
  }
  if (!Array.isArray(hostedValue.components) || hostedValue.components.length === 0) {
    hostedValue.components = ["main"];
  }
  if (!Array.isArray(hostedValue.architectures) || hostedValue.architectures.length === 0) {
    hostedValue.architectures = ["amd64", "all"];
  }
  refreshEnabledUi.value = false;
}

watch(selectedType, (newType) => {
  if (!isCreate.value) {
    return;
  }
  if (newType === "Proxy") {
    const snapshot = hosted.value;
    value.value = {
      type: "proxy",
      config: {
        ...defaultDebProxyConfig(),
        layout: { type: "dists", config: snapshot },
      },
    };
  } else {
    value.value = hosted.value;
  }
  normalize();
});

watch(selectedLayout, (newLayout) => {
  if (!isCreate.value) {
    return;
  }
  if (!isDebProxyConfig(value.value)) {
    return;
  }
  if (newLayout === "flat") {
    value.value.config.layout = { type: "flat", config: { distribution: "./", architectures: [] } };
  } else {
    value.value.config.layout = { type: "dists", config: hosted.value };
  }
  normalize();
});

watch(selectedRefreshType, (newType) => {
  if (!isDebProxyConfig(value.value)) {
    return;
  }
  const current = refresh.value;
  if (newType === "cron") {
    refresh.value = {
      ...current,
      schedule: { type: "cron", config: { expression: refreshCronExpression.value } },
    };
  } else {
    refresh.value = {
      ...current,
      schedule: { type: "interval_seconds", config: { interval_seconds: refreshIntervalSeconds.value } },
    };
  }
});

async function load() {
  if (!props.repository) {
    return;
  }
  try {
    suppressRefreshEnabledAutosave.value = true;
    const response = await http.get(`/api/repository/${props.repository}/config/deb`);
    value.value = response.data ?? defaultDebConfig();
    normalize();
    await loadRefreshStatus();
  } catch (error) {
    console.error(error);
  } finally {
    suppressRefreshEnabledAutosave.value = false;
  }
}

async function loadRefreshStatus() {
  if (!props.repository) {
    return;
  }
  if (!isDebProxyConfig(value.value)) {
    refreshStatus.value = null;
    return;
  }
  try {
    const response = await http.get(`/api/repository/${props.repository}/deb/refresh/status`);
    refreshStatus.value = response.data ?? null;
  } catch (error) {
    refreshStatus.value = null;
  }
}

async function save() {
  if (!props.repository) {
    return;
  }
  try {
    await http.put(`/api/repository/${props.repository}/config/deb`, value.value);
    await loadRefreshStatus();
  } catch (error) {
    console.error(error);
  }
}

async function refreshMirror() {
  if (!props.repository) {
    return;
  }
  manualRefreshNotice.value = null;
  isManualRefreshing.value = true;
  try {
    await http.post(`/api/repository/${props.repository}/deb/refresh`);
    manualRefreshNotice.value = { type: "success", message: "Mirror refresh started." };
  } catch (error: any) {
    const status = error?.response?.status;
    if (status === 409) {
      manualRefreshNotice.value = { type: "info", message: "Mirror refresh is already running." };
    } else {
      manualRefreshNotice.value = { type: "error", message: "Failed to start mirror refresh." };
    }
  } finally {
    isManualRefreshing.value = false;
    await loadRefreshStatus();
  }
}

onMounted(() => {
  normalize();
  load();
});
</script>

<style scoped lang="scss">
.deb-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.deb-config__refresh-controls {
  position: relative;
  z-index: 2;
}

.hint {
  font-size: 0.85rem;
  opacity: 0.8;
  margin-top: -0.75rem;
}
</style>
