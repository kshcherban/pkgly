<template>
  <form class="ruby-config" @submit.prevent="save">
    <DropDown
      v-model="selectedType"
      :options="typeOptions"
      :disabled="!isCreate"
      class="full-width"
      required
    >Repository Type</DropDown>

    <div v-if="isProxy" class="proxy-config">
      <TextInput
        v-model="upstreamUrl"
        required
        placeholder="https://rubygems.org"
      >Upstream URL</TextInput>

      <TextInput
        v-model="revalidationTtlSeconds"
        type="text"
        inputmode="numeric"
        pattern="\\d*"
        placeholder="300"
      >Revalidation TTL (seconds)</TextInput>

      <ProxyCacheNotice class="mt-2" />
    </div>

    <SubmitButton
      v-if="!isCreate"
      :block="false"
      prepend-icon="mdi-content-save">
      Save
    </SubmitButton>
  </form>
</template>
<script setup lang="ts">
import { computed, defineProps, onMounted, ref, watch } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import http from "@/http";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import { defaultProxy, type RubyConfigType } from "./ruby";

const typeOptions = [
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

const value = defineModel<RubyConfigType>({
  default: { type: "Hosted" },
});

const selectedType = ref<string>(value.value?.type ?? "Hosted");
const isCreate = computed(() => !props.repository);
const isProxy = computed(() => value.value?.type === "Proxy");

function normalizeValue() {
  if (!value.value || typeof value.value !== "object") {
    value.value = { type: "Hosted" };
  }
  if (value.value?.type === "Proxy") {
    const config = value.value.config ?? defaultProxy();
    value.value = {
      type: "Proxy",
      config: {
        upstream_url: config.upstream_url,
        revalidation_ttl_seconds: config.revalidation_ttl_seconds,
      },
    };
    selectedType.value = "Proxy";
    return;
  }
  value.value = { type: "Hosted" };
  selectedType.value = "Hosted";
}

function ensureProxyConfig() {
  if (value.value?.type !== "Proxy") {
    value.value = {
      type: "Proxy",
      config: defaultProxy(),
    };
    return;
  }
  if (!value.value.config || typeof value.value.config !== "object") {
    value.value = {
      type: "Proxy",
      config: defaultProxy(),
    };
  }
}

normalizeValue();

watch(selectedType, (newType) => {
  if (newType === "Proxy") {
    ensureProxyConfig();
  } else {
    value.value = { type: "Hosted" };
  }
});

const upstreamUrl = computed<string>({
  get: () => (value.value?.type === "Proxy" ? value.value.config.upstream_url : ""),
  set: (val: string) => {
    ensureProxyConfig();
    if (value.value?.type === "Proxy") {
      value.value.config.upstream_url = val;
    }
  },
});

const revalidationTtlSeconds = computed<string>({
  get: () =>
    value.value?.type === "Proxy" && value.value.config.revalidation_ttl_seconds !== undefined
      ? value.value.config.revalidation_ttl_seconds.toString()
      : "",
  set: (val: string) => {
    ensureProxyConfig();
    if (value.value?.type !== "Proxy") {
      return;
    }

    const trimmed = val.trim();
    if (trimmed === "") {
      value.value.config.revalidation_ttl_seconds = undefined;
      return;
    }

    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return;
    }
    value.value.config.revalidation_ttl_seconds = Math.floor(parsed);
  },
});

async function load() {
  if (!props.repository) {
    return;
  }
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/ruby`);
    value.value = response.data;
    normalizeValue();
  } catch (error) {
    console.error(error);
  }
}

async function save() {
  if (!props.repository) {
    return;
  }
  try {
    await http.put(`/api/repository/${props.repository}/config/ruby`, value.value);
  } catch (error) {
    console.error(error);
  }
}

onMounted(() => {
  if (!value.value) {
    value.value = { type: "Hosted" };
  }
  load();
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.ruby-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.full-width {
  width: 100%;
}
.proxy-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
</style>

