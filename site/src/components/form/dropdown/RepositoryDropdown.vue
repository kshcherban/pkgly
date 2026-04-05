<template>
  <DropDown
    id="repository-dropdown"
    class="repository-dropdown"
    v-model="value"
    :options="repositoryEntries" />
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { type RepositoryWithStorageName } from "@/types/repository";
import { useRepositoryStore } from "@/stores/repositories";
import DropDown from "@/components/form/dropdown/DropDown.vue";

const repositories = ref<RepositoryWithStorageName[]>([]);
const repoStore = useRepositoryStore();
repoStore.getRepositories().then((repos) => {
  repositories.value = repos;
});

const repositoryEntries = computed(() => {
  return repositories.value.map((repo) => ({
    label: `${repo.name} (${repo.storage_name})`,
    value: repo.id,
  }));
});

const value = defineModel<string>({
  required: true,
});
</script>

<style scoped lang="scss">
.repository-dropdown {
  width: 100%;
}
</style>
