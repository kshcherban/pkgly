<template>
  <main class="install-page">
    <form
      class="install-form"
      data-testid="install-form"
      @submit.prevent="install">
      <h1 class="install-form__title">Install Pkgly</h1>

      <div
        class="install-form__field"
        data-testid="install-field">
        <TextInput
          id="username"
          v-model="input.username"
          autocomplete="username"
          required
          placeholder="admin"
          >Username</TextInput
        >
      </div>

      <div
        class="install-form__field"
        data-testid="install-field">
        <PasswordInput
          id="password"
          v-model="input.password"
          required
          :newPassword="true"
          >Password</PasswordInput
        >
      </div>

      <SubmitButton
        class="install-form__submit"
        :disabled="installing || formValid !== ''"
        :title="installButtonTitle()">
        Install
      </SubmitButton>
    </form>
  </main>
</template>
<script setup lang="ts">
import SubmitButton from "@/components/form/SubmitButton.vue";
import PasswordInput from "@/components/form/text/PasswordInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import http from "@/http";
import router from "@/router";
import { siteStore } from "@/stores/site";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref } from "vue";
const input = ref({
  username: "",
  password: "",
});
const installing = ref(false);
const site = siteStore();
const alerts = useAlertsStore();
function installButtonTitle() {
  if (installing.value) {
    return "Installing...";
  }
  return formValid.value === "" ? "Install" : formValid.value;
}
const formValid = computed(() => {
  if (input.value.username === "") {
    return "Username is required.";
  }
  if (input.value.password === "") {
    return "Password is required.";
  }
  return "";
});
async function install() {
  if (installing.value || formValid.value !== "") {
    return;
  }
  const newUser = {
    username: input.value.username,
    password: input.value.password,
  };
  const install = {
    user: newUser,
  };
  installing.value = true;
  try {
    const response = await http.post("/api/install", install);
    if (response.status === 204) {
      alerts.success("Pkgly installed", "Redirecting to login…");
      await site.getInfo();
      await router.replace({ name: "login" });
    }
  } catch (error: any) {
    console.error("Install failed", error);
    if (error?.response?.status === 404) {
      alerts.error("Pkgly already installed", "Redirecting to login…");
      await router.replace({ name: "login" });
      return;
    }
    alerts.error("Install error", "An error occurred while installing the application.");
  } finally {
    installing.value = false;
  }
}
</script>
<style lang="scss" scoped>
.install-page {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  padding: 2rem 1rem;
  background: linear-gradient(180deg, rgba(30, 136, 229, 0.05), rgba(30, 136, 229, 0));
}

.install-form {
  width: min(100%, 480px);
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
  padding: 2.5rem 2rem;
  border-radius: 1rem;
  background: var(--nr-background-primary, #fff);
  box-shadow: 0 20px 45px rgba(15, 23, 42, 0.12);
}

.install-form__title {
  margin: 0;
  font-size: 1.75rem;
  font-weight: 600;
  text-align: center;
  color: var(--nr-text-color, #0f172a);
}

.install-form__field {
  width: 100%;
  display: flex;
  flex-direction: column;
}

.install-form__row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}

.install-form__submit {
  margin-top: 0.5rem;

  :deep(button) {
    width: 100%;
    font-size: 1rem;
    padding: 0.9rem 1rem;
    border-radius: 0.75rem;
  }
}

@media (max-width: 720px) {
  .install-form {
    padding: 2rem 1.5rem;
  }

  .install-form__row {
    grid-template-columns: 1fr;
  }
}
</style>
