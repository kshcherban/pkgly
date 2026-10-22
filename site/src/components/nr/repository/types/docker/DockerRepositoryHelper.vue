<!-- ABOUTME: Shows Docker CLI setup guidance for hosted and proxy repositories. -->
<!-- ABOUTME: Reflects whether pulls require authentication in the proxy instructions. -->
<template>
  <section class="docker-helper">
    <h2>Docker Registry</h2>
    <p v-if="isProxy">
      Pull images through this proxy cache.
      <template v-if="repository.auth_enabled">Authenticate once, then pull as usual.</template>
      <template v-else>Pull anonymously using the command below.</template>
    </p>
    <p v-else>
      Push images to this hosted registry. Authenticate once, tag your image, then push.
    </p>
    <CodeMenu :snippets="snippets" defaultTab="docker-proxy" />
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import CodeMenu from "@/components/core/code/CodeMenu.vue";
import type { RepositoryWithStorageName } from "@/types/repository";
import { createDockerSetupSnippets } from "./DockerRepositoryHelpers";

const props = defineProps<{ repository: RepositoryWithStorageName }>();

const isProxy = computed(() => props.repository.repository_kind?.toLowerCase() === "proxy");
const snippets = computed(() => createDockerSetupSnippets(props.repository));
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.docker-helper {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;

  h2 {
    margin: 0;
    color: $primary;
  }
}
</style>
