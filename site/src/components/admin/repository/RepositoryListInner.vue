<template>
  <div id="repositoryBox">
    <div id="headerBar">
      <h2>Repositories</h2>
      <div class="search-control">
        <input
          type="text"
          id="nameSearch"
          v-model="searchValue"
          autofocus
          placeholder="Search repositories or storage"
          aria-label="Search repositories or storage" />
        <button
          v-if="searchValue.length"
          type="button"
          class="search-control__clear"
          data-testid="repository-search-clear"
          aria-label="Clear repository search"
          @click="clearSearch">
          ×
        </button>
      </div>
    </div>
    <div
      id="repositories"
      class="betterScroll">
      <div
        class="row"
        id="header">
        <div
          :class="['col', { sorted: sortBy === 'id' }]"
          @click="sortBy = 'id'"
          title="Sort by ID">
          ID #
        </div>
        <div
          :class="['col', { sorted: sortBy === 'name' }]"
          @click="sortBy = 'name'"
          title="Sort by Name">
          Name
        </div>
        <div
          :class="['col', { sorted: sortBy === 'storage-type' }]"
          @click="sortBy = 'storage-type'"
          title="Sort by Storage Type">
          Storage Name
        </div>
        <div :class="['col']">Repository Type</div>
        <div :class="['col']">Kind</div>
        <div :class="['col']">Auth</div>
        <div :class="['col']">Storage</div>
        <div :class="['col']">Active</div>
        <div :class="['col']">Usage Updated</div>
      </div>
      <div
        class="row item"
        v-for="repository in filteredTable"
        :key="repository.id"
        @click="
          router.push({
            name: 'AdminViewRepository',
            params: { id: repository.id },
          })
        ">
        <div class="col">{{ repository.id }}</div>
        <div
          class="col"
          :title="repository.name">
          {{ repository.name }}
        </div>
        <div
          class="col"
          :title="repository.storage_name">
          {{ repository.storage_name }}
        </div>
        <div class="col">{{ repositoryTypeLabel(repository) }}</div>
        <div class="col">
          <v-chip
            size="small"
            :color="kindColor(repository)"
            variant="tonal">
            {{ repositoryKindLabel(repository) }}
          </v-chip>
        </div>
        <div class="col">{{ repository.auth_enabled ? 'On' : 'Off' }}</div>
        <div class="col">{{ formatBytes(repository.storage_usage_bytes) }}</div>
        <div class="col">{{ repository.active }}</div>
        <div class="col">{{ formatUpdatedAt(repository.storage_usage_updated_at) }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import router from "@/router";
import type { RepositoryWithStorageName } from "@/types/repository";
import { computed, ref, type PropType } from "vue";
const searchValue = ref<string>("");

const props = defineProps({
  repositories: Array as PropType<RepositoryWithStorageName[]>,
});
const sortBy = ref<string>("id");

function formatBytes(bytes?: number | null): string {
  if (bytes === null || bytes === undefined) {
    return "—";
  }
  if (bytes === 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  return `${value.toFixed(exponent === 0 ? 0 : 2)} ${units[exponent]}`;
}

function formatUpdatedAt(timestamp?: string | null): string {
  if (!timestamp) {
    return "—";
  }
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return "—";
  }
  return date.toLocaleString();
}

function sortList(a: RepositoryWithStorageName, b: RepositoryWithStorageName) {
  switch (sortBy.value) {
    case "id":
      return a.name.localeCompare(b.name);
    case "name":
      return a.name.localeCompare(b.name);

    default:
      return 0;
  }
}

function repositoryKindLabel(repo: RepositoryWithStorageName) {
  const kind = (repo.repository_kind ?? "hosted").toLowerCase();
  if (kind === "proxy") return "Proxy";
  if (kind === "virtual") return "Virtual";
  return "Hosted";
}

function repositoryTypeLabel(repo: RepositoryWithStorageName) {
  const kind = repositoryKindLabel(repo).toLowerCase();
  const type = repo.repository_type.toLowerCase();
  if (type === "docker" || kind === "proxy" || kind === "virtual") {
    return `${type} (${kind})`;
  }
  return repo.repository_type;
}

function kindColor(repo: RepositoryWithStorageName) {
  const kind = repositoryKindLabel(repo);
  if (kind === "Proxy") return "primary";
  if (kind === "Virtual") return "#b388ff";
  return "default";
}
const filteredTable = computed(() => {
  if (props.repositories == undefined) {
    return [];
  }
  const users = props.repositories.map((user) => user);
  return users.sort(sortList);
});

function clearSearch() {
  searchValue.value = "";
}
</script>
<style scoped lang="scss">
@use "@/assets/styles/theme" as *;
#headerBar {
  display: flex;
  justify-content: space-between;
  padding: 1rem;
  background-color: $primary-30;
  .search-control {
    position: relative;
    display: flex;
    align-items: center;
    width: 25%;

    input {
      width: 100%;
      padding-right: 2rem;
    }

    .search-control__clear {
      position: absolute;
      right: 0.5rem;
      background: transparent;
      border: none;
      cursor: pointer;
      font-size: 1.25rem;
      line-height: 1;
      color: $primary-400;

      &:hover {
        color: $accent;
      }
    }
  }
}
@media screen and (max-width: 1200px) {
  #headerBar {
    .search-control {
      width: 50%;
    }
  }
}
@media screen and (max-width: 800px) {
  #headerBar {
    display: flex;
    flex-direction: column;
    .search-control {
      width: 100%;
    }
  }
}
#storages {
  background-color: $primary-50;
}

#header {
  .col {
    font-weight: bold;
    &:hover {
      cursor: pointer;
      color: $accent;
      transition: all 0.3s ease;
    }
  }
}
.row {
  display: grid;
  grid-template-columns: 1fr 0.6fr 0.6fr 0.5fr 0.4fr 0.5fr 0.3fr 0.7fr;
  grid-template-rows: auto;
  .col {
    padding: 1rem;
    border-bottom: 1px solid $primary-30;
  }
}
.item {
  cursor: pointer;
  &:hover {
    background-color: $primary-30;
    transition: all 0.3s ease;
    .col {
      color: $accent;
      transition: all 0.3s ease;
    }
  }
}
</style>
