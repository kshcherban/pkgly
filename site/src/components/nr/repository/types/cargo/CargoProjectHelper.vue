<template>
  <section class="cargo-helper">
    <h2>Cargo Registry</h2>
    <p>
      Add the following to your <code>.cargo/config.toml</code> (or
      <code>CARGO_HOME/config</code>) to enable the registry:
    </p>

    <pre><code>[registries.{{ registryName }}]
index = "{{ indexUrl }}"
api = "{{ apiUrl }}"
</code></pre>

    <p>
      Publish with <code>cargo publish --registry {{ registryName }}</code>. Generate a Pkgly
      API token and run <code>cargo login --registry {{ registryName }}</code> to authenticate.
    </p>
  </section>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { apiURL } from "@/config";
import type { RepositoryWithStorageName } from "@/types/repository";

const props = defineProps<{ repository: RepositoryWithStorageName }>();

const trimmed = computed(() => apiURL.replace(/\/$/, ""));
const base = computed(
  () => `${trimmed.value}/repositories/${props.repository.storage_name}/${props.repository.name}`,
);

const registryName = computed(() => props.repository.name);
const indexUrl = computed(() => `${base.value}/index`);
const apiUrl = computed(() => `${base.value}/api/v1`);
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.cargo-helper {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;

  h2 {
    margin: 0;
    color: $primary;
  }

  pre {
    background: rgba(0, 0, 0, 0.08);
    padding: 0.75rem;
    border-radius: 0.5rem;
    font-size: 0.9rem;
    overflow-x: auto;
  }

  code {
    font-family: "Fira Code", "Source Code Pro", monospace;
  }
}
</style>
