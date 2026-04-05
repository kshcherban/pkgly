<template>
  <v-container class="login-container">
    <v-row justify="center" align="center">
      <v-col cols="12" sm="8" md="6" lg="4" xl="3">
        <v-card class="elevation-4" max-width="450">
          <v-card-title class="text-center pa-6">
            <div class="d-flex flex-column align-center">
              <v-avatar
                :image="'/logo.svg'"
                size="64"
                class="mb-4" />
              <span class="text-h4 font-weight-medium text-primary">Pkgly</span>
              <span class="text-body-1 text-medium-emphasis mt-1">Sign in to your account</span>
            </div>
          </v-card-title>

          <v-card-text class="pa-6 pt-0">
            <!-- Primary Federated Login (SSO or first OAuth) -->
            <div
              v-if="primaryFederated"
              class="mb-4">
              <v-btn
                block
                size="large"
                color="primary"
                variant="flat"
                @click="federatedHandler(primaryFederated)"
                class="text-none">
                <v-icon start>mdi-login</v-icon>
                {{ primaryFederated.label }}
              </v-btn>
              <p
                v-if="showAutoProvisionMessage"
                class="text-center text-caption text-medium-emphasis mt-2">
                Your account will be created automatically on first login.
              </p>
            </div>

            <!-- Secondary OAuth Providers -->
            <div
              v-if="secondaryProviders.length > 0"
              class="mb-4">
              <v-btn
                v-for="provider in secondaryProviders"
                :key="provider.provider"
                block
                size="large"
                color="secondary"
                variant="outlined"
                @click="startOAuth(provider.provider)"
                class="text-none mb-2">
                <v-icon start>mdi-account-circle</v-icon>
                Sign in with {{ providerLabel(provider.provider) }}
              </v-btn>
            </div>

            <!-- Separator -->
            <div
              v-if="hasFederatedLogin"
              class="my-6">
              <v-divider>
                <span class="text-caption text-medium-emphasis px-2">or</span>
              </v-divider>
            </div>

            <!-- Local Login Form -->
            <v-form @submit.prevent="login" class="login-form">
              <v-alert
                v-if="failedLogin"
                type="error"
                variant="tonal"
                class="mb-4">
                Invalid username or password
              </v-alert>

              <v-text-field
                v-model="input.email_or_username"
                label="Username or Email"
                autocomplete="username"
                autocapitalize="false"
                variant="outlined"
                prepend-inner-icon="mdi-account"
                required
                autofocus
                class="mb-4" />

              <v-text-field
                v-model="input.password"
                label="Password"
                :type="showPassword ? 'text' : 'password'"
                autocomplete="current-password"
                variant="outlined"
                prepend-inner-icon="mdi-lock"
                :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                @click:append-inner="showPassword = !showPassword"
                required
                class="mb-4" />

              <div class="text-end mb-4">
                <router-link
                  to="/forgot-password"
                  class="text-primary text-decoration-none">
                  Forgot Password?
                </router-link>
              </div>

              <v-btn
                type="submit"
                block
                size="large"
                color="primary"
                variant="flat"
                class="text-none">
                Login
              </v-btn>
            </v-form>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </v-container>
</template>
<script setup lang="ts">
import http from "@/http";
import router from "@/router";
import { sessionStore } from "@/stores/session";
import { siteStore } from "@/stores/site";
import { useAlertsStore } from "@/stores/alerts";
import type { InstanceOAuth2Provider } from "@/types/base";
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
const failedLogin = ref(false);
const showPassword = ref(false);
const input = ref({
  email_or_username: "",
  password: "",
});
const session = sessionStore();
const site = siteStore();
const route = useRoute();
const redirectTarget = computed(() => {
  const redirect = route.query.redirect;
  if (typeof redirect === "string" && redirect.startsWith("/") && !redirect.startsWith("//")) {
    return redirect;
  }
  return "/";
});
const oauthProviders = computed<InstanceOAuth2Provider[]>(() => {
  const providers = site.siteInfo?.oauth2?.providers;
  return Array.isArray(providers) ? providers : [];
});
const ssoEnabled = computed(() => Boolean(site.siteInfo?.sso));
const hasFederatedLogin = computed(
  () => ssoEnabled.value || oauthProviders.value.length > 0,
);
const ssoButtonText = computed(
  () => site.siteInfo?.sso?.login_button_text ?? "Sign in with SSO",
);
type FederatedTarget =
  | { kind: "sso"; label: string }
  | { kind: "oauth"; label: string; provider: InstanceOAuth2Provider };

const primaryFederated = computed<FederatedTarget | null>(() => {
  if (ssoEnabled.value) {
    return { kind: "sso", label: ssoButtonText.value };
  }
  const firstProvider = oauthProviders.value[0];
  if (!firstProvider) {
    return null;
  }
  return {
    kind: "oauth",
    label: `Sign in with ${providerLabel(firstProvider.provider)}`,
    provider: firstProvider,
  };
});
const secondaryProviders = computed<InstanceOAuth2Provider[]>(() => {
  if (ssoEnabled.value) {
    return oauthProviders.value;
  }
  return oauthProviders.value.slice(1);
});
const showAutoProvisionMessage = computed(() => {
  if (ssoEnabled.value) {
    return site.siteInfo?.sso?.auto_create_users ?? false;
  }
  return site.siteInfo?.oauth2?.auto_create_users ?? false;
});
const alerts = useAlertsStore();

async function login() {
  try {
    const response = await http.post("/api/user/login", input.value);
    session.login(response.data);
    router.push(redirectTarget.value);
  } catch (error: any) {
    const status = error?.response?.status;
    if (status === 401) {
      failedLogin.value = true;
    } else {
      console.error(error);
      alerts.error("Login failed", "An error occurred while trying to login.");
    }
  }
}
function startSso() {
  const loginPath = site.siteInfo?.sso?.login_path ?? "/api/user/sso/login";
  try {
    const ssoUrl = resolveUrl(loginPath);
    ssoUrl.searchParams.set("redirect", redirectTarget.value);

    const providerUrl = site.siteInfo?.sso?.provider_login_url ?? undefined;
    if (providerUrl && providerUrl !== "") {
      const providerTarget = resolveUrl(providerUrl);
      const redirectParam =
        site.siteInfo?.sso?.provider_redirect_param?.trim() ?? "redirect";
      providerTarget.searchParams.set(redirectParam, ssoUrl.toString());
      window.location.href = providerTarget.toString();
    } else {
      window.location.href = ssoUrl.toString();
    }
  } catch (error) {
    console.error("Invalid SSO configuration", error);
  }
}

function resolveUrl(target: string): URL {
  if (target.startsWith("http://") || target.startsWith("https://")) {
    return new URL(target);
  }
  const normalized = target.startsWith("/") ? target : `/${target}`;
  return new URL(normalized, window.location.origin);
}

function sanitizeBase(path: string): string {
  return path.endsWith("/") ? path.slice(0, -1) : path;
}

function startOAuth(provider: string) {
  const basePath = site.siteInfo?.oauth2?.login_path ?? "/api/user/oauth2/login";
  const targetPath = `${sanitizeBase(basePath)}/${provider}`;
  const oauthUrl = resolveUrl(targetPath);
  oauthUrl.searchParams.set("redirect", redirectTarget.value);
  window.location.href = oauthUrl.toString();
}

function providerLabel(provider: string): string {
  switch (provider.toLowerCase()) {
    case "google":
      return "Google";
    case "microsoft":
      return "Microsoft";
    default:
      return provider.charAt(0).toUpperCase() + provider.slice(1);
  }
}

function federatedHandler(target: FederatedTarget) {
  if (target.kind === "sso") {
    startSso();
  } else {
    startOAuth(target.provider.provider);
  }
}

onMounted(async () => {
  await site.getInfo();
});
</script>
<style scoped lang="scss">
.login-container {
  min-height: 100vh;
  background-color: rgb(var(--v-theme-background));
}

// Ensure proper color contrast for links
:deep(.v-btn .v-btn__content) {
  color: rgb(var(--v-theme-on-primary));
}

// Style the login form for better appearance
.login-form {
  .v-text-field {
    .v-field__outline {
      border-color: rgba(var(--v-theme-outline-variant), 0.5);
    }

    &:focus-within .v-field__outline {
      border-color: rgb(var(--v-theme-primary));
    }
  }
}
</style>
