<template>
  <v-container class="pa-6">
    <!-- Loading State -->
    <v-card v-if="loading && !error" class="text-center py-8">
      <v-progress-circular indeterminate color="primary" size="48" />
      <div class="mt-4 text-medium-emphasis">Loading storages…</div>
    </v-card>

    <!-- Error State -->
    <v-alert
      v-else-if="error"
      type="error"
      variant="tonal"
      prominent>
      {{ error }}
    </v-alert>

    <!-- Storage List -->
    <v-card v-else-if="storages.length >= 1" class="elevation-0">
      <v-card-title class="d-flex align-center pa-4">
        <span class="text-h6">Storages</span>
        <v-spacer />
        <v-btn
          color="primary"
          prepend-icon="mdi-plus"
          :to="{ name: 'StorageCreate' }"
          variant="flat">
          Create Storage
        </v-btn>
      </v-card-title>

      <v-data-table
        :headers="headers"
        :items="tableItems"
        :loading="loading"
        item-value="id"
        @click:row="handleRowClick"
        class="elevation-0 storage-table">

        <template v-slot:item.active="{ value }">
          <v-chip
            :color="value ? 'success' : 'error'"
            :variant="value ? 'flat' : 'outlined'"
            size="small">
            {{ value ? 'Active' : 'Inactive' }}
          </v-chip>
        </template>

        <template v-slot:no-data>
          <div class="pa-4 text-center text-medium-emphasis">
            No storages found.
          </div>
        </template>
      </v-data-table>
    </v-card>

    <!-- Empty State -->
    <v-card
      v-else
      class="text-center py-8"
      variant="outlined">
      <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-database</v-icon>
      <div class="text-h6 text-medium-emphasis mb-2">No storages found</div>
      <div class="text-body-2 text-medium-emphasis mb-4">
        Create your first storage to get started.
      </div>
      <v-btn
        color="primary"
        prepend-icon="mdi-plus"
        :to="{ name: 'StorageCreate' }"
        variant="flat">
        Create Storage
      </v-btn>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import { useRouter } from "vue-router";
import { computed, ref } from "vue";
import type { DataTableHeader } from "vuetify";
import { useRepositoryStore } from "@/stores/repositories";
import type { StorageItem } from "@/components/nr/storage/storageTypes";

const router = useRouter();
const storages = ref<StorageItem[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const repositoriesTypesStore = useRepositoryStore();

// Define table headers
const headers: DataTableHeader[] = [
  {
    title: 'ID #',
    key: 'id',
    value: 'id',
    sortable: true,
  },
  {
    title: 'Name',
    key: 'name',
    value: 'name',
    sortable: true,
  },
  {
    title: 'Storage Type',
    key: 'storage_type',
    value: 'storage_type',
    sortable: true,
  },
  {
    title: 'Active',
    key: 'active',
    value: 'active',
    sortable: true,
  },
];

// Convert storages to v-data-table format
const tableItems = computed(() => {
  return storages.value.map((storage) => ({
    id: storage.id,
    name: storage.name,
    storage_type: storage.storage_type,
    active: storage.active,
  }));
});

async function fetchStorages() {
  loading.value = true;
  error.value = null;
  try {
    const response = await repositoriesTypesStore.getStorages();
    storages.value = response;
  } catch (err) {
    console.error(err);
    error.value = "Failed to fetch storages";
  } finally {
    loading.value = false;
  }
}

type DataTableRow = { item: { id?: string | number } | { raw?: { id?: string | number } } };

// Handle row click navigation
function handleRowClick(_event: MouseEvent, row: DataTableRow) {
  const candidate = (row.item as { raw?: { id?: string | number }; id?: string | number }) ?? {};
  const id = candidate.raw?.id ?? candidate.id;
  if (!id) {
    return;
  }
  router.push({
    name: 'ViewStorage',
    params: { id },
  });
}

fetchStorages();
</script>

<style scoped lang="scss">
// Ensure v-data-table respects theme colors and add cursor pointer for rows
:deep(.v-data-table) {
  .v-data-table__th {
    color: var(--nr-text-primary);
    background-color: var(--nr-table-header-background);
  }

  .v-data-table__td {
    color: var(--nr-text-primary);
  }

  .v-data-table__tr {
    cursor: pointer;

    &:hover {
      background-color: var(--nr-table-row-hover);
    }
  }
}

// Storage table specific styling
.storage-table {
  tbody tr:hover {
    cursor: pointer;
  }
}
</style>
