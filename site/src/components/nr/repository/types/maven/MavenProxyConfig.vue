<template>
  <section class="maven-proxy">
    <div class="maven-proxy__header">
      <h3 class="text-subtitle-1 font-weight-medium mb-2">Upstream Routes</h3>
      <p class="text-body-2 text-medium-emphasis">
        Routes are tried in order to fetch artifacts from remote Maven repositories.
      </p>
    </div>

    <div class="maven-proxy__routes" v-auto-animate>
      <div
        v-for="(route, index) in value.routes"
        :key="`${route.url}-${index}`"
        class="maven-proxy__route">
        <v-row dense>
          <v-col cols="12" md="4">
            <TextInput
              v-model="route.url"
              required
              placeholder="https://repo1.maven.org/maven2/">
              Upstream URL
            </TextInput>
          </v-col>
          <v-col cols="12" md="4">
            <TextInput
              v-model="route.name"
              placeholder="Maven Central">
              Display Name
            </TextInput>
          </v-col>
          <v-col
            cols="12"
            md="4"
            class="d-flex align-end justify-end">
            <v-btn
            color="error"
            variant="flat"
            class="route-action text-none danger-hover h-56"
            :disabled="value.routes.length <= 1"
            prepend-icon="mdi-delete"
            @click="removeRoute(index)">
            Remove
          </v-btn>
          </v-col>
        </v-row>
      </div>
    </div>

    <v-btn
      color="primary"
      variant="tonal"
      class="route-add text-none align-self-start mt-2"
      prepend-icon="mdi-plus"
      @click="addRoute">
      Add Route
    </v-btn>
  </section>
</template>

<script setup lang="ts">
import { reactive } from "vue";
import TextInput from "@/components/form/text/TextInput.vue";
import { defaultProxy, type MavenProxyRoute, type MavenProxyConfigType } from "./maven";

const value = defineModel<MavenProxyConfigType>({
  required: true,
});

if (!value.value || !Array.isArray(value.value.routes)) {
  value.value = defaultProxy();
} else if (value.value.routes.length === 0) {
  value.value = defaultProxy();
}

function removeRoute(index: number) {
  if (index < 0 || index >= value.value.routes.length) {
    return;
  }
  value.value = {
    ...value.value,
    routes: value.value.routes.filter((_, i) => i !== index),
  };
}

function addRoute() {
  value.value = {
    ...value.value,
    routes: [
      ...value.value.routes,
      {
        url: "",
        name: "",
      },
    ],
  };
}
</script>

<style scoped lang="scss">
.maven-proxy {
  display: flex;
  flex-direction: column;
  gap: 1rem;

  &__routes {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  &__route,
  &__add {
    padding: 0;
    border-radius: 0;
    background-color: transparent;
    border: none;
    box-shadow: none;
  }

  :deep(.h-56) {
    min-height: 56px;
  }

  :deep(.route-action) {
    --v-btn-height: 48px;
    margin: 0;
    width: 100%;
    height: 48px;
    min-height: 48px;
    max-height: 48px;
    align-self: flex-start;
  }

  .route-add {
    align-self: flex-start;
  }
}
</style>
