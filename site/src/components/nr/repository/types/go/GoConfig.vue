<template>
  <form class="go-config" @submit.prevent="save">
    <DropDown
      v-model="value.type"
      :options="typeOptions"
      :required="true"
      :disabled="!isCreate"
      class="full-width"
    >
      Repository Type
    </DropDown>

    <div v-if="value.type === 'Proxy'" class="proxy-configuration">
      <h4>Proxy Configuration</h4>

      <div class="cache-settings">
        <NumberInput
          v-model="proxyConfig.go_module_cache_ttl!"
          :min="0"
          :max="86400"
          placeholder="3600"
        >
          Module Cache TTL (seconds)
        </NumberInput>
      </div>

      <div class="proxy-routes">
        <div class="routes-header">
          <div>
            <h5>Upstream Proxy Routes</h5>
            <p class="routes-description">Configure upstream Go module proxies in priority order</p>
          </div>
        </div>

        <div v-if="proxyConfig.routes.length === 0" class="no-routes">
          <p>No proxy routes configured. Add at least one route to enable proxy functionality.</p>
        </div>

        <div
          v-for="(route, index) in proxyConfig.routes"
          :key="index"
          class="route-row"
        >
          <TextInput
            v-model="route.url"
            :required="true"
            placeholder="https://proxy.golang.org"
            :error="getRouteErrors(index, 'url')"
          >
            Proxy URL
          </TextInput>

          <TextInput
            v-model="route.name"
            placeholder="Optional display name"
            :error="getRouteErrors(index, 'name')"
          >
            Display Name
          </TextInput>

          <NumberInput
            v-model="route.priority!"
            :min="0"
            :max="100"
            placeholder="0"
            :error="getRouteErrors(index, 'priority')"
          >
            Priority
          </NumberInput>

          <v-btn
            color="error"
            variant="flat"
            class="route-action text-none danger-hover"
            type="button"
            prepend-icon="mdi-delete"
            @click="removeRoute(index)"
            :disabled="proxyConfig.routes.length <= 1">
            Remove
          </v-btn>
        </div>

        <v-btn
          color="primary"
          variant="tonal"
          class="text-none route-add align-self-start"
          type="button"
          prepend-icon="mdi-plus"
          @click="addRoute">
          Add Route
        </v-btn>
      </div>

      <div class="proxy-info">
        <div class="info-box">
          <h6>How Go Proxy Works</h6>
          <p>
            Go modules will be fetched from the configured proxy routes in priority order
            (higher priority = tried first). If a route fails, the next route will be tried.
          </p>
          <p>
            The official Go proxy (proxy.golang.org) is recommended as the primary route.
          </p>
        </div>
      </div>
      <ProxyCacheNotice class="mt-4" />
    </div>

    <div v-else-if="value.type === 'Hosted'" class="hosted-configuration">
      <div class="info-box">
        <h6>Hosted Go Repository</h6>
        <p>
          This repository will host Go modules directly. Users can upload and download
          Go modules through this Pkgly instance.
        </p>
        <p>
          Hosted mode supports the standard Go module protocol including version lists,
          module info, go.mod files, and module zip downloads.
        </p>
      </div>
    </div>

    <SubmitButton
      v-if="!isCreate"
      :block="false"
      :disabled="hasErrors"
      class="go-config__submit"
      prepend-icon="mdi-content-save">
      Save Configuration
    </SubmitButton>
  </form>
</template>

<script setup lang="ts">
import { computed, defineProps, onMounted, ref, watch } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import NumberInput from "@/components/form/NumberInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import http from "@/http";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import type { GoConfigType, GoProxyConfigType, GoProxyRoute } from "./go";
import { defaultProxy, validateGoConfig, validateProxyRoute } from "./go";

const props = defineProps<{
  settingName: string;
  repository?: string;
}>();

const value = defineModel<GoConfigType>({
  required: true,
  default: () => ({
    type: "Proxy",
    config: defaultProxy()
  })
});

const isCreate = computed(() => !props.repository);
const isProxy = computed(() => value.value.type === "Proxy");

const typeOptions = [
  { value: "Hosted", label: "Hosted - Store Go modules directly" },
  { value: "Proxy", label: "Proxy - Fetch from upstream Go proxies" },
];

const proxyConfig = computed<GoProxyConfigType>({
  get: () => {
    if (value.value.type === "Proxy") {
      if (!value.value.config) {
        // Initialize with default proxy config
        return defaultProxy();
      }
      return value.value.config;
    }
    // For Hosted repositories, return default proxy config for the form UI
    // but it won't be included in the final submission
    return defaultProxy();
  },
  set: (config) => {
    if (value.value.type === "Proxy") {
      value.value.config = config;
    }
    // For Hosted repositories, don't store the config
  },
});

const routeErrors = ref<Record<string, string[]>>({});

const getRouteErrors = (index: number, field: string): string | undefined => {
  const key = `${index}-${field}`;
  return routeErrors.value[key]?.[0];
};

const hasErrors = computed(() => {
  const errors = validateGoConfig(value.value);
  return errors.length > 0;
});

// Watch for type changes to initialize config properly
watch(() => value.value.type, (newType) => {
  if (newType === "Proxy" && !('config' in value.value)) {
    (value.value as any).config = defaultProxy();
  }
}, { immediate: true });

function addRoute() {
  if (value.value.type === "Proxy") {
    if (!proxyConfig.value.routes) {
      proxyConfig.value.routes = [];
    }
    const newRoute: GoProxyRoute = {
      url: "https://proxy.golang.org",
      name: "New Proxy Route",
      priority: proxyConfig.value.routes.length * 10,
    };
    proxyConfig.value.routes.push(newRoute);
    validateAllRoutes();
  }
}

function removeRoute(index: number) {
  if (value.value.type === "Proxy" && proxyConfig.value.routes.length > 1) {
    proxyConfig.value.routes.splice(index, 1);
    validateAllRoutes();
  }
}

function validateAllRoutes() {
  if (value.value.type === "Proxy" && proxyConfig.value.routes) {
    const newErrors: Record<string, string[]> = {};

    proxyConfig.value.routes.forEach((route, index) => {
      const errors = validateProxyRoute(route);
      errors.forEach((error) => {
        if (error.includes("URL")) {
          newErrors[`${index}-url`] = [error];
        } else if (error.includes("Priority")) {
          newErrors[`${index}-priority`] = [error];
        } else {
          newErrors[`${index}-name`] = [error];
        }
      });
    });

    routeErrors.value = newErrors;
  }
}

function load() {
  if (props.repository) {
    http
      .get(`/api/repository/${props.repository}/config/go`)
      .then((response) => {
        if (response.data) {
          value.value = response.data;
        }
      })
      .catch((error) => {
        console.error("Failed to load Go configuration:", error);
      });
  }
}

function save() {
  if (props.repository) {
    // Prepare the config in the correct format for the backend
    let configToSave: any;

    if (value.value.type === "Proxy") {
      configToSave = {
        type: "Proxy",
        config: value.value.config
      };
    } else {
      // For Hosted, only send the type without config
      configToSave = {
        type: "Hosted"
      };
    }

    http
      .put(`/api/repository/${props.repository}/config/go`, configToSave)
      .then(() => {
        // Success feedback could be added here
      })
      .catch((error) => {
        console.error("Failed to save Go configuration:", error);
      });
  }
}

// Watch for changes in proxy configuration to validate
watch(
  () => proxyConfig.value.routes,
  () => {
    validateAllRoutes();
  },
  { deep: true }
);

onMounted(() => {
  load();
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.go-config {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  width: 100%;
  max-width: none;
}

.full-width {
  width: 100%;
}

.proxy-configuration,
.hosted-configuration {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  width: 100%;
}

.cache-settings {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.proxy-routes {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  width: 100%;
}

.routes-header {
  display: flex;
  justify-content: flex-start;
  align-items: center;
  margin-bottom: 0.5rem;

  div {
    h5 {
      margin: 0;
      font-size: 1rem;
      font-weight: 600;
    }

    .routes-description {
      margin: 0;
      font-size: 0.85rem;
      color: var(--color-text-secondary);
    }
  }
}

.no-routes {
  padding: 1rem;
  background-color: var(--color-background-secondary);
  border-radius: var(--border-radius);
  border: 1px dashed var(--color-border);

  p {
    margin: 0;
    color: var(--color-text-secondary);
    text-align: center;
  }
}

.route-row {
  display: grid;
  gap: 1rem;
  grid-template-columns: minmax(260px, 2fr) minmax(200px, 1.2fr) 140px 140px;
  align-items: stretch;
  padding: 1rem;
  background-color: var(--color-background-secondary);
  border-radius: var(--border-radius);
  border: 1px solid var(--color-border);
  min-height: 80px;
  width: 100%;

  @media (max-width: 1200px) {
    grid-template-columns: minmax(220px, 1.5fr) minmax(180px, 1fr) 140px 140px;
  }

  @media (max-width: 1024px) {
    grid-template-columns: repeat(2, minmax(220px, 1fr));
    gap: 0.75rem;
  }

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
    gap: 0.5rem;
    min-height: auto;
  }
}

.proxy-info {
  margin-top: 1rem;
}

.info-box {
  padding: 1rem;
  background-color: var(--color-background-info);
  border-radius: var(--border-radius);
  border-left: 4px solid var(--color-info);

  h6 {
    margin: 0 0 0.5rem 0;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--color-info);
  }

  p {
    margin: 0.25rem 0;
    font-size: 0.85rem;
    color: var(--color-text-secondary);

    &:last-child {
      margin-bottom: 0;
    }
  }
}

// Ensure input fields have proper sizing for URLs
.route-row {
  :deep(.text-input),
  :deep(.number-input) {
    min-width: 0;
    width: 100%;

    input {
      width: 100%;
      min-height: 40px;
      padding: 0.5rem 0.75rem;
      font-size: 0.9rem;
    }
  }

  :deep(.v-btn) {
    &.route-action {
      --v-btn-height: 48px;
      margin: 0;
      width: 100%;
      height: 48px;
      min-height: 48px;
      max-height: 48px;
      align-self: start;
      justify-content: center;
    }
    min-height: 56px;
    white-space: nowrap;
    align-self: end;
  }
}

.route-add {
  margin-top: 0.25rem;
}

.go-config__submit {
  align-self: flex-start;

  :deep(.submit-button) {
    min-width: 12rem;
  }
}

// Break out of parent form constraints - these styles will be applied globally
// through the main component to override the CreateRepositoryView constraints
</style>
