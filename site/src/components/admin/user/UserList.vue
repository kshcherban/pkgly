<template>
  <v-card>
    <v-card-title class="d-flex align-center pa-4">
      <span class="text-h6">Users</span>
      <v-spacer />
      <v-text-field
        v-model="searchValue"
        placeholder="Search by Name, Username, Email"
        prepend-inner-icon="mdi-magnify"
        variant="outlined"
        density="compact"
        clearable
        @click:clear="clearSearch"
        hide-details
        style="max-width: 300px;"
        autofocus />
    </v-card-title>

    <v-data-table
      :headers="headers"
      :items="tableItems"
      :search="searchValue"
      item-value="id"
      @click:row="handleRowClick"
      class="elevation-0 user-table">

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
          No users found matching your search.
        </div>
      </template>
    </v-data-table>
  </v-card>
</template>

<script setup lang="ts">
import router from "@/router";
import type { UserResponseType } from "@/types/base";
import { computed, ref, type PropType } from "vue";
import type { DataTableHeader } from "vuetify";

const searchValue = ref<string>("");

const props = defineProps({
  users: Array as PropType<UserResponseType[]>,
});

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
    title: 'Username',
    key: 'username',
    value: 'username',
    sortable: true,
  },
  {
    title: 'Status',
    key: 'active',
    value: 'active',
    sortable: true,
  },
];

// Convert users to v-data-table format
const tableItems = computed(() => {
  if (!props.users) {
    return [];
  }
  return props.users.map((user) => ({
    id: user.id,
    name: user.name,
    username: user.username,
    active: user.active,
    email: user.email, // Include for search functionality
  }));
});

type DataTableRow = { item: { id?: string | number } | { raw?: { id?: string | number } } };

// Handle row click navigation
function handleRowClick(_event: MouseEvent, row: DataTableRow) {
  const candidate = (row.item as { raw?: { id?: string | number }; id?: string | number }) ?? {};
  const id = candidate.raw?.id ?? candidate.id;
  if (!id) {
    return;
  }
  router.push(`/admin/user/${id}`);
}

function clearSearch() {
  searchValue.value = "";
}
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

// User table specific styling
.user-table {
  tbody tr:hover {
    cursor: pointer;
  }
}
</style>
