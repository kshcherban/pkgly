<template>
  <div id="storagesBox">
    <div id="headerBar">
      <h2>Storages</h2>
      <div class="search-control">
        <input
          type="text"
          id="nameSearch"
          v-model="searchValue"
          autofocus
          placeholder="Search by Name, Username, or Primary Email Address"
          aria-label="Search storages" />
        <button
          v-if="searchValue.length"
          type="button"
          class="search-control__clear"
          data-testid="storage-search-clear"
          aria-label="Clear storage search"
          @click="clearSearch">
          ×
        </button>
      </div>
    </div>
    <div
      id="storages"
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
          Storage Type
        </div>
        <div :class="['col']">Active</div>
      </div>
      <div
        class="row item"
        v-for="storage in filteredTable"
        :key="storage.id"
        @click="
          router.push({
            name: 'ViewStorage',
            params: { id: storage.id },
          })
        ">
        <div class="col">{{ storage.id }}</div>
        <div
          class="col"
          :title="storage.name">
          {{ storage.name }}
        </div>
        <div
          class="col"
          :title="storage.storage_type">
          {{ storage.storage_type }}
        </div>
        <div class="col">{{ storage.active }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import router from "@/router";

import { computed, ref, type PropType } from "vue";
import type { StorageItem } from "./storageTypes";
const searchValue = ref<string>("");

const props = defineProps({
  storages: Array as PropType<StorageItem[]>,
});
const sortBy = ref<string>("id");

function sortList(a: StorageItem, b: StorageItem) {
  switch (sortBy.value) {
    case "id":
      return a.name.localeCompare(b.name);
    case "name":
      return a.name.localeCompare(b.name);

    default:
      return 0;
  }
}
const filteredTable = computed(() => {
  if (props.storages == undefined) {
    return [];
  }
  const users = props.storages.map((user) => user);
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
  grid-template-columns: 1fr 0.5fr 0.5fr 0.5fr;
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
