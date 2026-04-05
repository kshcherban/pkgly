<template>
  <form class="php-config" data-testid="php-config-card" @submit.prevent="save">
    <DropDown
      v-model="selectedType"
      :options="phpTypes"
      :disabled="!isCreate"
      class="full-width"
      required
    >Repository Type</DropDown>

    <div v-if="isProxy" class="proxy-routes">
      <div
        v-for="(route, index) in proxyRoutes"
        :key="index"
        class="route-row"
      >
        <TextInput
          v-model="route.url"
          placeholder="https://repo.packagist.org"
          required
        >Upstream URL</TextInput>
        <TextInput
          v-model="route.name"
          placeholder="Optional label"
        >Display Name</TextInput>
        <v-btn
          color="error"
          variant="flat"
          class="route-action text-none danger-hover"
          type="button"
          prepend-icon="mdi-delete"
          @click="removeRoute(index)"
        >Remove</v-btn>
      </div>
      <v-btn
        color="primary"
        variant="tonal"
        class="text-none align-self-start"
        type="button"
        prepend-icon="mdi-plus"
        @click="addRoute">
        Add Route
      </v-btn>
      <ProxyCacheNotice class="mt-2" />
    </div>

    <v-alert
      v-if="isHosted"
      variant="tonal"
      type="info"
      class="mt-4"
      density="comfortable">
      Hosted mode stores Composer packages you upload. Switch to Proxy to cache Packagist (or another
      upstream) locally.
    </v-alert>

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
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import http from "@/http";
import { defaultProxy, type PhpConfigType } from "./php";

const phpTypes = [
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

const value = defineModel<PhpConfigType>({
  default: { type: "Hosted" },
});

const selectedType = ref<string>(value.value?.type ?? "Hosted");
const isCreate = computed(() => !props.repository);
const isProxy = computed(() => value.value?.type === "Proxy");
const isHosted = computed(() => value.value?.type === "Hosted");
const proxyRoutes = computed(() => {
  if (value.value?.type !== "Proxy") {
    return [] as ReturnType<typeof defaultProxy>["routes"];
  }
  return value.value.config.routes;
});

function normalizeValue() {
  if (!value.value || typeof value.value !== "object") {
    value.value = { type: "Hosted" };
  }
  if (value.value?.type === "Proxy") {
    const routes = (value.value.config?.routes ?? defaultProxy().routes).map((route) => ({
      url: route.url,
      name: route.name ?? undefined,
    }));
    value.value = {
      type: "Proxy",
      config: {
        routes: routes.length > 0 ? routes : defaultProxy().routes,
      },
    };
    selectedType.value = "Proxy";
    return;
  }
  value.value = { type: "Hosted" };
  selectedType.value = "Hosted";
}

normalizeValue();

watch(selectedType, (newType) => {
  if (newType === "Proxy") {
    if (value.value?.type !== "Proxy") {
      value.value = {
        type: "Proxy",
        config: defaultProxy(),
      };
    } else if (value.value.config.routes.length === 0) {
      value.value.config.routes = defaultProxy().routes;
    }
  } else {
    value.value = { type: "Hosted" };
  }
});

function ensureProxyConfig() {
  if (value.value?.type !== "Proxy") {
    value.value = {
      type: "Proxy",
      config: defaultProxy(),
    };
  }
}

function addRoute() {
  ensureProxyConfig();
  if (value.value?.type === "Proxy") {
    value.value.config.routes.push({ url: "", name: "" });
  }
}

function removeRoute(index: number) {
  if (value.value?.type === "Proxy") {
    value.value.config.routes.splice(index, 1);
    if (value.value.config.routes.length === 0) {
      value.value.config.routes = defaultProxy().routes;
    }
  }
}

async function load() {
  if (!props.repository) {
    return;
  }
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/php`);
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
    await http.put(
      `/api/repository/${props.repository}/config/${props.settingName ?? "php"}`,
      value.value,
    );
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

.php-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.full-width {
  width: 100%;
}
.proxy-routes {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.route-row {
  display: grid;
  column-gap: 0.75rem;
  row-gap: 0.5rem;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  align-items: stretch;

  :deep(.route-action) {
    --v-btn-height: 48px;
    margin: 0;
    width: 100%;
    height: 48px;
    min-height: 48px;
    max-height: 48px;
    align-self: start;
    justify-self: stretch;
  }
}
</style>
