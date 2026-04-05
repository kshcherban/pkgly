<template>
  <section class="npm-helper">
    <h2>NPM Repository</h2>
    <p>
      Configure your client with
      <code>npm config set registry {{ registryUrl }}</code>
      or append <code>--registry={{ registryUrl }}</code> to installs.
    </p>
    <p>Authentication works with Pkgly user credentials or generated auth tokens.</p>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { apiURL } from "@/config";
import type { RepositoryWithStorageName } from "@/types/repository";

const props = defineProps<{ repository: RepositoryWithStorageName }>();

const registryUrl = computed(() => {
  const trimmed = apiURL.replace(/\/$/, "");
  return `${trimmed}/repositories/${props.repository.storage_name}/${props.repository.name}/`;
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.npm-helper {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;

  h2 {
    margin: 0;
    color: $primary;
  }

  code {
    background: rgba(0, 0, 0, 0.25);
    padding: 0.15rem 0.35rem;
    border-radius: 0.25rem;
    font-size: 0.95em;
  }
}
</style>
