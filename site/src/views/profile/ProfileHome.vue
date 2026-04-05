<template>
  <v-container v-if="user" class="pa-6">
    <v-card>
      <v-card-title class="d-flex align-center pa-4">
        <v-avatar
          color="primary"
          size="48"
          class="mr-3">
          <v-icon>mdi-account</v-icon>
        </v-avatar>
        <div>
          <h1 class="text-h4 font-weight-medium">Welcome, {{ user.name }}</h1>
          <p class="text-body-1 text-medium-emphasis">Profile Information</p>
        </div>
      </v-card-title>

      <v-divider />

      <v-card-text class="pa-4">
        <v-row>
          <v-col cols="12" md="6">
            <div class="text-body-2 text-medium-emphasis mb-1">Email</div>
            <div class="text-body-1 font-weight-medium">{{ user.email }}</div>
          </v-col>
          <v-col cols="12" md="6">
            <div class="text-body-2 text-medium-emphasis mb-1">Username</div>
            <div class="text-body-1 font-weight-medium">{{ user.username }}</div>
          </v-col>
        </v-row>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import { sessionStore } from "@/stores/session";

const session = sessionStore();
const user = session.user;
</script>
