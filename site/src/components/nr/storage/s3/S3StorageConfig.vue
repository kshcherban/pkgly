<template>
  <section class="s3-config">
    <TwoByFormBox>
      <TextInput
        id="s3-bucket-name"
        v-model="model.bucket_name"
        required
        autocomplete="off"
        spellcheck="false">
        Bucket Name
      </TextInput>
      <div v-if="!useCustomEndpoint" class="stacked-field">
        <v-autocomplete
          id="s3-region"
          v-model="regionSelection"
          :items="regionOptions"
          item-title="label"
          item-value="value"
          label="AWS Region"
          variant="outlined"
          density="comfortable"
          autocomplete="off"
          clearable
          auto-select-first
          no-data-text="No matching regions" />
        <p v-if="regionsLoading" class="helper">Loading regions…</p>
        <p v-else-if="regionError" class="helper error">{{ regionError }}</p>
        <p v-else class="helper">
          Pick the AWS region that matches your bucket. Use the custom endpoint option for
          non-AWS providers.
        </p>
      </div>
    </TwoByFormBox>

    <SwitchInput
      id="s3-use-custom-endpoint"
      v-model="useCustomEndpoint">
      Use custom endpoint
      <template #comment>
        Enable this for MinIO, Ceph, DigitalOcean Spaces, or any S3-compatible gateway with a custom
        URL.
      </template>
    </SwitchInput>

    <TwoByFormBox v-if="useCustomEndpoint">
      <TextInput
        id="s3-endpoint"
        v-model="model.endpoint"
        :required="useCustomEndpoint"
        placeholder="https://minio.internal.example.com"
        autocomplete="off"
        spellcheck="false">
        Endpoint URL
      </TextInput>
      <TextInput
        id="s3-custom-region"
        v-model="model.custom_region"
        placeholder="Optional label (onprem-us1)"
        autocomplete="off"
        spellcheck="false">
        Custom Region Name
      </TextInput>
    </TwoByFormBox>

    <TwoByFormBox>
      <TextInput
        id="s3-access-key"
        v-model="model.credentials.access_key"
        autocomplete="off"
        spellcheck="false">
        Access Key
      </TextInput>
      <TextInput
        id="s3-secret-key"
        v-model="model.credentials.secret_key"
        type="password"
        autocomplete="new-password">
        Secret Key
      </TextInput>
    </TwoByFormBox>
    <p
      v-if="credentialHint"
      class="helper error">
      {{ credentialHint }}
    </p>
    <TextInput
      id="s3-session-token"
      v-model="model.credentials.session_token"
      autocomplete="off"
      spellcheck="false">
      Session Token (optional)
    </TextInput>

    <TwoByFormBox>
      <TextInput
        id="s3-role-arn"
        v-model="model.credentials.role_arn"
        autocomplete="off"
        spellcheck="false"
        placeholder="arn:aws:iam::123456789012:role/pkgly">
        Role ARN (optional)
      </TextInput>
      <TextInput
        id="s3-role-session-name"
        v-model="model.credentials.role_session_name"
        autocomplete="off"
        spellcheck="false"
        placeholder="pkgly-ci">
        Role Session Name (optional)
      </TextInput>
    </TwoByFormBox>
    <TextInput
      id="s3-external-id"
      v-model="model.credentials.external_id"
      autocomplete="off"
      spellcheck="false">
      External ID (optional)
    </TextInput>

    <SwitchInput
      id="s3-path-style"
      v-model="model.path_style">
      Force path-style requests
      <template #comment>
        Keep enabled for MinIO and most custom gateways. Disable if AWS requires virtual-hosted
        style (bucket.s3.amazonaws.com).
      </template>
    </SwitchInput>

    <SwitchInput
      id="s3-cache-enabled"
      v-model="model.cache.enabled">
      Enable local disk cache
      <template #comment>
        Stores frequently-read artifacts on this node to avoid repeated downloads from S3. Configure
        the path and byte limit to match local disk capacity.
      </template>
    </SwitchInput>

    <TwoByFormBox v-if="model.cache.enabled">
      <TextInput
        id="s3-cache-path"
        v-model="model.cache.path"
        autocomplete="off"
        spellcheck="false"
        placeholder="/var/lib/pkgly-cache/s3">
        Cache directory
      </TextInput>
      <div style="display: flex; gap: 1rem; align-items: flex-start;">
        <TextInput
          id="s3-cache-size"
          v-model="cacheSizeDisplay"
          type="number"
          step="0.1"
          min="0"
          style="flex-grow: 1;">
          Max Size
        </TextInput>
        <DropDown
          id="s3-cache-unit"
          v-model="selectedUnit"
          :options="unitOptions"
          style="width: 100px; flex-shrink: 0;">
          Unit
        </DropDown>
      </div>
    </TwoByFormBox>
    <TextInput
      v-if="model.cache.enabled"
      id="s3-cache-max-entries"
      v-model="maxEntriesString"
      type="number"
      min="1"
      step="1">
      Max cached entries
    </TextInput>
  </section>
</template>

<script setup lang="ts">
import DropDown from "@/components/form/dropdown/DropDown.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import TwoByFormBox from "@/components/form/TwoByFormBox.vue";
import http from "@/http";
import { computed, onMounted, ref, watch, watchEffect, type Ref } from "vue";
import type { S3StorageSettings } from "@/components/nr/storage/storageTypes";

const model = defineModel<S3StorageSettings>({
  default: () => ({
    bucket_name: "",
    region: undefined,
    custom_region: undefined,
    endpoint: undefined,
    credentials: {
      access_key: "",
      secret_key: "",
    },
    path_style: true,
    cache: {
      enabled: false,
      path: "",
      max_bytes: 536870912,
      max_entries: 2048,
    },
  }),
}) as Ref<S3StorageSettings>;

const ensureModel = (): S3StorageSettings => {
  if (!model.value) {
    model.value = {
      bucket_name: "",
      region: undefined,
      custom_region: undefined,
      endpoint: undefined,
      credentials: {
        access_key: "",
        secret_key: "",
      },
      path_style: true,
      cache: {
        enabled: false,
        path: "",
        max_bytes: 536870912,
        max_entries: 2048,
      },
    };
  }
  return model.value;
};

const unitOptions = [
  { label: "MB", value: "MB" },
  { label: "GB", value: "GB" },
];

const multipliers = {
  MB: 1024 * 1024,
  GB: 1024 * 1024 * 1024,
} as const;

type Unit = keyof typeof multipliers;

const selectedUnit = ref<Unit>("MB");
const initBytes = ensureModel().cache.max_bytes;
if (initBytes > 0 && initBytes % multipliers.GB === 0) {
  selectedUnit.value = "GB";
}

const cacheSizeDisplay = computed({
  get: () => {
    const bytes = ensureModel().cache.max_bytes;
    const mult = multipliers[selectedUnit.value];
    const val = bytes / mult;
    return Number.isInteger(val) ? val.toString() : val.toFixed(2);
  },
  set: (val: string) => {
    const num = parseFloat(val);
    if (!isNaN(num)) {
      ensureModel().cache.max_bytes = Math.floor(num * multipliers[selectedUnit.value]);
    }
  },
});

const maxEntriesString = computed({
  get: () => String(ensureModel().cache.max_entries),
  set: (value: string) => {
    const num = parseInt(value, 10);
    ensureModel().cache.max_entries = isNaN(num) ? 0 : num;
  },
});

const regionsLoading = ref(false);
const regionError = ref<string | null>(null);
const regionOptions = ref<{ label: string; value: string }[]>([]);
const useCustomEndpoint = ref(Boolean(ensureModel().endpoint));

const regionSelection = computed({
  get: () => ensureModel().region ?? "",
  set: (value: string) => {
    ensureModel().region = value || undefined;
  },
});

watchEffect(() => {
  const state = ensureModel();
  state.credentials ??= {};
  state.credentials.access_key ??= "";
  state.credentials.secret_key ??= "";
  state.credentials.session_token ??= "";
  state.credentials.role_arn ??= "";
  state.credentials.role_session_name ??= "";
  state.credentials.external_id ??= "";
  state.cache ??= {
    enabled: false,
    path: "",
    max_bytes: 536870912,
    max_entries: 2048,
  };
  state.cache.path ??= "";
  if (typeof state.cache.max_bytes !== "number" || state.cache.max_bytes <= 0) {
    state.cache.max_bytes = 536870912;
  }
  if (typeof state.cache.max_entries !== "number" || state.cache.max_entries <= 0) {
    state.cache.max_entries = 2048;
  }
  if (typeof state.path_style !== "boolean") {
    state.path_style = true;
  }
  state.cache ??= {
    enabled: false,
    path: "",
    max_bytes: 536870912,
    max_entries: 2048,
  };
  state.cache.path ??= "";
  if (typeof state.cache.max_bytes !== "number" || state.cache.max_bytes <= 0) {
    state.cache.max_bytes = 536870912;
  }
  if (typeof state.cache.max_entries !== "number" || state.cache.max_entries <= 0) {
    state.cache.max_entries = 2048;
  }
});

watch(
  () => useCustomEndpoint.value,
  (enabled) => {
    const state = ensureModel();
    if (enabled) {
      state.region = undefined;
    } else {
      state.endpoint = undefined;
      state.custom_region = undefined;
      if (!state.region) {
        const firstRegion = regionOptions.value[0];
        if (firstRegion) {
          state.region = firstRegion.value;
        }
      }
    }
  },
  { immediate: true },
);

async function loadRegions() {
  regionsLoading.value = true;
  try {
    const response = await http.get<string[]>("/api/storage/s3/regions");
    regionOptions.value = response.data.map((region) => ({
      label: formatRegionId(region),
      value: region,
    }));
    regionError.value = null;
    if (!useCustomEndpoint.value) {
      const state = ensureModel();
      if (!state.region) {
        const firstRegion = regionOptions.value[0];
        if (firstRegion) {
          state.region = firstRegion.value;
        }
      }
    }
  } catch (error) {
    console.error("Failed to load S3 regions", error);
    regionError.value = "Unable to load regions. Verify your session and try again.";
  } finally {
    regionsLoading.value = false;
  }
}

function formatRegionId(value: string): string {
  if (value.includes("-")) {
    return value.toLowerCase();
  }
  return value.match(/[A-Z][a-z]+|\d+/g)?.join("-").toLowerCase() ?? value;
}

onMounted(() => {
  loadRegions();
});

const credentialHint = computed(() => {
  const state = ensureModel();
  const access = state.credentials.access_key?.trim();
  const secret = state.credentials.secret_key?.trim();
  const role = state.credentials.role_arn?.trim();
  if ((access && !secret) || (!access && secret)) {
    return "Provide both access and secret keys, or leave both blank.";
  }
  if (access && secret && role) {
    return "Access keys and IAM role are mutually exclusive. Choose one method.";
  }
  return null;
});
</script>

<style scoped lang="scss">
.s3-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.stacked-field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.helper {
  font-size: 0.875rem;
  color: var(--nr-text-secondary);
  margin: 0;
}

.helper.error {
  color: var(--nr-error, #c62828);
}
</style>
