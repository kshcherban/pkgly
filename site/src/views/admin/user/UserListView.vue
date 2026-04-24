<template>
  <v-container class="pa-6">
    <v-alert
      v-if="error"
      type="error"
      variant="tonal"
      prominent>
      {{ error }}
    </v-alert>

    <v-card
      v-else-if="loading"
      class="text-center py-8"
      variant="flat">
      <v-progress-circular indeterminate color="primary" size="48" />
      <div class="mt-4 text-medium-emphasis">Loading users…</div>
    </v-card>

    <template v-else>
      <div
        v-if="users.length > 0"
        class="d-flex justify-end mb-4">
        <v-btn
          data-testid="create-user-button"
          color="primary"
          prepend-icon="mdi-account-plus"
          :to="{ name: 'UserCreate' }"
          variant="flat">
          Create User
        </v-btn>
      </div>

      <UserList
        v-if="users.length > 0"
        :users="users" />

      <v-card
        v-else
        class="text-center py-8"
        variant="outlined">
        <v-icon color="medium-emphasis" size="48" class="mb-2">mdi-account-multiple</v-icon>
        <div class="text-h6 text-medium-emphasis mb-2">No users found</div>
        <div class="text-body-2 text-medium-emphasis mb-4">
          Create a user to manage access and permissions.
        </div>
        <v-btn
          color="primary"
          prepend-icon="mdi-account-plus"
          :to="{ name: 'UserCreate' }"
          variant="flat">
          Create User
        </v-btn>
      </v-card>
    </template>
  </v-container>
</template>

<script setup lang="ts">
import UserList from "@/components/admin/user/UserList.vue";
import http from "@/http";
import type { UserResponseType } from "@/types/base";
import { onMounted, ref } from "vue";

const users = ref<UserResponseType[]>([]);
const loading = ref(true);
const error = ref<string | null>(null);

async function fetchUsers() {
  loading.value = true;
  error.value = null;
  try {
    const response = await http.get<UserResponseType[]>("/api/user-management/list");
    users.value = response.data;
  } catch (err) {
    console.error(err);
    error.value = "Failed to fetch users";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void fetchUsers();
});
</script>
