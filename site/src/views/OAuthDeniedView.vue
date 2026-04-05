<template>
  <main class="page">
    <section class="card">
      <h1>Access Denied</h1>
      <p class="summary">
        We couldn’t sign you in with your organization account because this user doesn’t have the
        necessary Pkgly permissions.
      </p>
      <p class="details">
        {{ reasonMessage }}
      </p>
      <p class="details">
        If you believe this is a mistake, contact your Pkgly administrator so they can review
        the OAuth group-to-role mapping or update your access.
      </p>
      <div class="actions">
        <router-link
          class="primary"
          to="/login">
          Back to Login
        </router-link>
      </div>
    </section>
  </main>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";

const route = useRoute();

const reasonMessage = computed(() => {
  const reason = (route.query.reason as string | undefined)?.toLowerCase();
  switch (reason) {
    case "inactive":
      return "Your Pkgly account has been deactivated. Please ask an administrator to reactivate it.";
    case "no_account":
      return "Your Pkgly account was not found and automatic provisioning is disabled.";
    case "no_roles":
    default:
      return "Your identity provider account is not mapped to any Pkgly roles.";
  }
});
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;
.page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2rem;
  background: $background-70;
}

.card {
  max-width: 520px;
  width: 100%;
  background: $background;
  border-radius: 1rem;
  padding: 2.5rem 2rem;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

h1 {
  margin: 0;
  font-size: 2rem;
  color: $primary;
}

.summary {
  font-weight: 600;
  margin: 0;
}

.details {
  margin: 0;
  color: $text-50;
}

.actions {
  margin-top: 1rem;
  display: flex;
  justify-content: flex-end;
}

.primary {
  background: $primary-70;
  color: $background;
  padding: 0.65rem 1.25rem;
  border-radius: 0.75rem;
  font-weight: 600;
  text-decoration: none;
  transition: background 0.2s ease-in-out;

  &:hover {
    background: $primary-90;
  }
}
</style>
