<template>
  <section class="s3-config">
    <TwoByFormBox>
      <TextInput
        id="s3-bucket-name-display"
        v-model="model.bucket_name"
        disabled>
        Bucket Name
      </TextInput>
      <TextInput
        id="s3-region-display"
        v-model="regionDisplay"
        disabled>
        Region / Endpoint Mode
      </TextInput>
    </TwoByFormBox>

    <TwoByFormBox v-if="model.endpoint">
      <TextInput
        id="s3-endpoint-display"
        v-model="endpointDisplay"
        disabled>
        Endpoint URL
      </TextInput>
      <TextInput
        id="s3-custom-region-display"
        v-model="customRegionDisplay"
        disabled>
        Custom Region Name
      </TextInput>
    </TwoByFormBox>

    <TwoByFormBox>
      <TextInput
        id="s3-access-key-display"
        v-model="model.credentials.access_key"
        disabled>
        Access Key
      </TextInput>
      <TextInput
        id="s3-secret-key-display"
        :model-value="maskedSecret"
        type="password"
        disabled>
        Secret Key
      </TextInput>
    </TwoByFormBox>

    <TextInput
      id="s3-session-token-display"
      :model-value="maskedSession"
      type="password"
      disabled>
      Session Token
    </TextInput>

    <TwoByFormBox>
      <TextInput
        id="s3-role-arn-display"
        v-model="model.credentials.role_arn"
        disabled>
        Role ARN
      </TextInput>
      <TextInput
        id="s3-role-session-display"
        v-model="model.credentials.role_session_name"
        disabled>
        Role Session Name
      </TextInput>
    </TwoByFormBox>

    <TextInput
      id="s3-external-id-display"
      v-model="model.credentials.external_id"
      disabled>
      External ID
    </TextInput>

    <TextInput
      id="s3-path-style-display"
      :model-value="model.path_style ? 'Path-style' : 'Virtual-hosted'"
      disabled>
      Addressing Mode
    </TextInput>

    <TextInput
      id="s3-cache-enabled-display"
      :model-value="model.cache.enabled ? 'Enabled' : 'Disabled'"
      disabled>
      Disk Cache
    </TextInput>

    <TwoByFormBox v-if="model.cache.enabled">
      <TextInput
        id="s3-cache-path-display"
        v-model="model.cache.path"
        disabled>
        Cache Directory
      </TextInput>
      <TextInput
        id="s3-cache-max-bytes-display"
        :model-value="formattedMaxSize"
        disabled>
        Max Size
      </TextInput>
    </TwoByFormBox>
    <TextInput
      v-if="model.cache.enabled"
      id="s3-cache-max-entries-display"
      :model-value="String(model.cache.max_entries)"
      disabled>
      Max Cached Entries
    </TextInput>
  </section>
</template>

<script setup lang="ts">
import TextInput from "@/components/form/text/TextInput.vue";
import TwoByFormBox from "@/components/form/TwoByFormBox.vue";
import { computed } from "vue";
import type { S3StorageSettings } from "@/components/nr/storage/storageTypes";

const model = defineModel<S3StorageSettings>({
  required: true,
});

if (!model.value.credentials) {
  model.value.credentials = {};
}
model.value.credentials.access_key ??= "";
model.value.credentials.secret_key ??= "";
model.value.credentials.session_token ??= "";
model.value.credentials.role_arn ??= "";
model.value.credentials.role_session_name ??= "";
model.value.credentials.external_id ??= "";
if (typeof model.value.path_style !== "boolean") {
  model.value.path_style = true;
}
model.value.cache ??= {
  enabled: false,
  path: "",
  max_bytes: 536870912,
  max_entries: 2048,
};
model.value.cache.path ??= "";
if (typeof model.value.cache.max_bytes !== "number") {
  model.value.cache.max_bytes = 536870912;
}
if (typeof model.value.cache.max_entries !== "number") {
  model.value.cache.max_entries = 2048;
}

const regionDisplay = computed({
  get: () => {
    if (model.value.endpoint) {
      return "Custom endpoint";
    }
    return model.value.region ?? "Region not set";
  },
  set: () => {},
});

const endpointDisplay = computed({
  get: () => model.value.endpoint ?? "",
  set: () => {},
});

const customRegionDisplay = computed({
  get: () => model.value.custom_region ?? "",
  set: () => {},
});

const formattedMaxSize = computed(() => {
  const bytes = model.value.cache.max_bytes;
  if (bytes >= 1024 * 1024 * 1024) {
    return (bytes / (1024 * 1024 * 1024)).toFixed(2) + " GB";
  }
  return (bytes / (1024 * 1024)).toFixed(2) + " MB";
});

const maskedSecret = computed(() => {
  const secret = model.value.credentials?.secret_key ?? "";
  if (!secret) {
    return "";
  }
  return "*".repeat(Math.min(secret.length, 12));
});

const maskedSession = computed(() => {
  const token = model.value.credentials?.session_token ?? "";
  if (!token) {
    return "";
  }
  return "*".repeat(Math.min(token.length, 12));
});
</script>

<style scoped lang="scss">
.s3-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
</style>
