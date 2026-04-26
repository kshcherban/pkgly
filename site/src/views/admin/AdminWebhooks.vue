<script setup lang="ts">
import SwitchInput from "@/components/form/SwitchInput.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import PasswordInput from "@/components/form/text/PasswordInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SpinnerElement from "@/components/spinner/SpinnerElement.vue";
import FloatingErrorBanner from "@/components/ui/FloatingErrorBanner.vue";
import http from "@/http";
import type {
  WebhookConfiguration,
  WebhookDeliveryStatus,
  WebhookEventType,
  WebhookUpdatePayload,
} from "@/types/base";
import { useAlertsStore } from "@/stores/alerts";
import { computed, onMounted, ref, watch } from "vue";
import { resolveRequestError } from "./resolveRequestError";

interface EditableWebhookHeader {
  id: string;
  name: string;
  value: string;
  configured: boolean;
}

interface EditableWebhookConfiguration {
  id: string | null;
  name: string;
  enabled: boolean;
  target_url: string;
  events: WebhookEventType[];
  headers: EditableWebhookHeader[];
}

const webhooksLoading = ref(true);
const webhooksSaving = ref(false);
const webhooksDeletingId = ref<string | null>(null);
const webhooks = ref<WebhookConfiguration[]>([]);
const webhookForm = ref<EditableWebhookConfiguration>(defaultWebhookForm());
const webhookInitialSignature = ref(JSON.stringify(toWebhookPayload(webhookForm.value)));

const alerts = useAlertsStore();

const hasWebhookChanges = computed(
  () => webhookInitialSignature.value !== JSON.stringify(toWebhookPayload(webhookForm.value)),
);
const isEditingWebhook = computed(() => Boolean(webhookForm.value.id));

const errorBanner = ref({
  visible: false,
  title: "",
  message: "",
});

const showError = (title: string, message: string) => {
  errorBanner.value.visible = true;
  errorBanner.value.title = title;
  errorBanner.value.message = message;
};

const resetError = () => {
  errorBanner.value.visible = false;
  errorBanner.value.title = "";
  errorBanner.value.message = "";
};

onMounted(async () => {
  await fetchWebhooks();
});

watch(
  webhookForm,
  () => {
    if (errorBanner.value.visible) {
      resetError();
    }
  },
  { deep: true },
);

function generateId(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return Math.random().toString(36).slice(2);
}

async function fetchWebhooks() {
  webhooksLoading.value = true;
  resetError();
  try {
    const response = await http.get<WebhookConfiguration[]>("/api/system/webhooks");
    webhooks.value = response.data;
    if (webhookForm.value.id) {
      const current = response.data.find((webhook) => webhook.id === webhookForm.value.id);
      if (current) {
        webhookForm.value = toWebhookEditable(current);
        webhookInitialSignature.value = JSON.stringify(toWebhookPayload(webhookForm.value));
      } else {
        startNewWebhook();
      }
    } else if (response.data.length > 0) {
      const [firstWebhook] = response.data;
      if (firstWebhook) {
        selectWebhook(firstWebhook);
      } else {
        startNewWebhook();
      }
    } else {
      startNewWebhook();
    }
  } catch (error) {
    const resolved = resolveRequestError(
      error,
      "Unable to load webhooks",
      "Check the server logs for more information.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    webhooksLoading.value = false;
  }
}

async function saveWebhookSettings() {
  if (webhooksSaving.value) {
    return;
  }
  resetError();

  if (!webhookForm.value.name.trim()) {
    showError("Webhook name required", "Provide a webhook name.");
    return;
  }
  if (!webhookForm.value.target_url.trim()) {
    showError("Target URL required", "Provide a destination URL for the webhook.");
    return;
  }
  if (webhookForm.value.events.length === 0) {
    showError("Select an event", "Choose at least one package event.");
    return;
  }

  for (const header of webhookForm.value.headers) {
    if (!header.name.trim()) {
      showError("Header name required", "Every configured header needs a name.");
      return;
    }
    if (!header.configured && !header.value.trim()) {
      showError(
        "Header value required",
        `Provide a value for header '${header.name.trim() || "(unnamed)"}'.`,
      );
      return;
    }
  }

  webhooksSaving.value = true;
  const payload = toWebhookPayload(webhookForm.value);

  try {
    if (webhookForm.value.id) {
      await http.put(`/api/system/webhooks/${webhookForm.value.id}`, payload);
      alerts.success("Webhook updated");
    } else {
      await http.post("/api/system/webhooks", payload);
      alerts.success("Webhook created");
    }
    await fetchWebhooks();
  } catch (error) {
    const resolved = resolveRequestError(
      error,
      "Unable to save webhook",
      "Check the server logs for more details.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    webhooksSaving.value = false;
  }
}

async function deleteWebhook(id: string) {
  if (webhooksDeletingId.value) {
    return;
  }
  resetError();
  webhooksDeletingId.value = id;
  try {
    await http.delete(`/api/system/webhooks/${id}`);
    alerts.success("Webhook deleted");
    await fetchWebhooks();
  } catch (error) {
    const resolved = resolveRequestError(
      error,
      "Unable to delete webhook",
      "Check the server logs for more details.",
    );
    console.error(resolved.debugMessage);
    showError(resolved.title, resolved.message);
  } finally {
    webhooksDeletingId.value = null;
  }
}

function resetWebhookSettings() {
  if (webhooksSaving.value) {
    return;
  }
  resetError();
  if (webhookForm.value.id) {
    const latest = webhooks.value.find((webhook) => webhook.id === webhookForm.value.id);
    if (latest) {
      selectWebhook(latest);
      return;
    }
  }
  startNewWebhook();
}

function defaultWebhookForm(): EditableWebhookConfiguration {
  return {
    id: null,
    name: "",
    enabled: true,
    target_url: "",
    events: ["package.published"],
    headers: [],
  };
}

function defaultWebhookHeader(): EditableWebhookHeader {
  return {
    id: generateId(),
    name: "",
    value: "",
    configured: false,
  };
}

function toWebhookEditable(settings: WebhookConfiguration): EditableWebhookConfiguration {
  return {
    id: settings.id,
    name: settings.name,
    enabled: settings.enabled,
    target_url: settings.target_url,
    events: [...settings.events],
    headers: (settings.headers ?? []).map((header) => ({
      id: generateId(),
      name: header.name,
      value: "",
      configured: header.configured,
    })),
  };
}

function toWebhookPayload(settings: EditableWebhookConfiguration): WebhookUpdatePayload {
  const headers = settings.headers
    .map((header) => ({
      name: header.name.trim(),
      value: header.value.trim() ? header.value.trim() : null,
      configured: header.configured,
    }))
    .filter((header) => header.name.length > 0);

  return {
    name: settings.name.trim(),
    enabled: settings.enabled,
    target_url: settings.target_url.trim(),
    events: Array.from(new Set(settings.events)),
    headers,
  };
}

function selectWebhook(webhook: WebhookConfiguration) {
  webhookForm.value = toWebhookEditable(webhook);
  webhookInitialSignature.value = JSON.stringify(toWebhookPayload(webhookForm.value));
}

function startNewWebhook() {
  webhookForm.value = defaultWebhookForm();
  webhookInitialSignature.value = JSON.stringify(toWebhookPayload(webhookForm.value));
}

function addWebhookHeader() {
  webhookForm.value.headers.push(defaultWebhookHeader());
}

function removeWebhookHeader(id: string) {
  webhookForm.value.headers = webhookForm.value.headers.filter((header) => header.id !== id);
}

function toggleWebhookEvent(event: WebhookEventType, checked: boolean) {
  if (checked) {
    if (!webhookForm.value.events.includes(event)) {
      webhookForm.value.events.push(event);
    }
    return;
  }
  webhookForm.value.events = webhookForm.value.events.filter((candidate) => candidate !== event);
}

function onWebhookEventToggle(event: WebhookEventType, inputEvent: Event) {
  const target = inputEvent.target as HTMLInputElement | null;
  toggleWebhookEvent(event, target?.checked ?? false);
}

function webhookStatusLabel(status?: WebhookDeliveryStatus | null): string {
  switch (status) {
    case "delivered":
      return "Delivered";
    case "failed":
      return "Failed";
    case "processing":
      return "In progress";
    case "pending":
      return "Queued";
    default:
      return "Never sent";
  }
}
</script>

<template>
  <main class="systemSettings">
    <FloatingErrorBanner
      :visible="errorBanner.visible"
      :title="errorBanner.title"
      :message="errorBanner.message"
      @close="resetError" />
    <h1>Webhooks</h1>

    <section class="card">
      <header>
        <h2>Package Webhooks</h2>
        <p>
          Send outbound notifications when packages are published or deleted. Deliveries are
          asynchronous and retried automatically.
        </p>
      </header>
      <SpinnerElement v-if="webhooksLoading" />
      <div
        v-else
        class="webhookSection">
        <div class="webhookList">
          <header class="providerSection__header">
            <div class="providerSection__title">
              <h3>Configured webhooks</h3>
              <p class="hint">Header values stay write-only after save.</p>
            </div>
            <v-btn
              variant="outlined"
              color="primary"
              prepend-icon="mdi-plus"
              @click="startNewWebhook">
              New webhook
            </v-btn>
          </header>

          <div
            v-if="webhooks.length === 0"
            class="emptyState">
            No package webhooks configured yet.
          </div>

          <button
            v-for="webhook in webhooks"
            :key="webhook.id"
            type="button"
            class="webhookListItem"
            :class="{ 'webhookListItem--active': webhook.id === webhookForm.id }"
            @click="selectWebhook(webhook)">
            <span class="webhookListItem__name">{{ webhook.name }}</span>
            <span class="webhookListItem__meta">
              {{ webhook.enabled ? "Enabled" : "Disabled" }} ·
              {{ webhookStatusLabel(webhook.last_delivery_status) }}
            </span>
          </button>
        </div>

        <form
          class="webhookForm"
          @submit.prevent="saveWebhookSettings">
          <div class="grid">
            <TextInput
              id="webhook-name"
              v-model="webhookForm.name"
              autocomplete="off"
              required>
              Webhook name
            </TextInput>

            <TextInput
              id="webhook-target-url"
              v-model="webhookForm.target_url"
              autocomplete="off"
              placeholder="https://example.com/webhooks/pkgly"
              required>
              Target URL
            </TextInput>
          </div>

          <SwitchInput
            id="webhook-enabled"
            v-model="webhookForm.enabled">
            Enable webhook
            <template #comment>
              Disabled webhooks remain saved but stop receiving new delivery jobs.
            </template>
          </SwitchInput>

          <div class="eventPicker">
            <label class="eventPicker__option">
              <input
                :checked="webhookForm.events.includes('package.published')"
                type="checkbox"
                @change="onWebhookEventToggle('package.published', $event)">
              <span>package.published</span>
            </label>
            <label class="eventPicker__option">
              <input
                :checked="webhookForm.events.includes('package.deleted')"
                type="checkbox"
                @change="onWebhookEventToggle('package.deleted', $event)">
              <span>package.deleted</span>
            </label>
          </div>

          <div class="providerSection">
            <header class="providerSection__header">
              <div class="providerSection__title">
                <h3>Custom headers</h3>
                <p class="hint">Leave an existing value blank to keep it unchanged.</p>
              </div>
              <v-btn
                variant="outlined"
                color="primary"
                prepend-icon="mdi-plus"
                @click.prevent="addWebhookHeader">
                Add header
              </v-btn>
            </header>

            <div
              v-if="webhookForm.headers.length === 0"
              class="emptyState">
              No custom headers configured.
            </div>

            <div
              v-for="header in webhookForm.headers"
              :key="header.id"
              class="mappingRow">
              <TextInput
                :id="`webhook-header-name-${header.id}`"
                v-model="header.name"
                autocomplete="off"
                required>
                Header name
              </TextInput>
              <PasswordInput
                :id="`webhook-header-value-${header.id}`"
                v-model="header.value"
                autocomplete="off"
                :placeholder="header.configured ? 'Leave blank to keep existing' : 'Header value'">
                Header value
              </PasswordInput>
              <v-btn
                variant="text"
                color="error"
                prepend-icon="mdi-delete"
                @click.prevent="removeWebhookHeader(header.id)">
                Remove
              </v-btn>
            </div>
          </div>

          <div
            v-if="isEditingWebhook"
            class="webhookStatus">
            <strong>Last delivery:</strong>
            {{ webhookStatusLabel(webhooks.find((item) => item.id === webhookForm.id)?.last_delivery_status) }}
            <span v-if="webhooks.find((item) => item.id === webhookForm.id)?.last_http_status">
              · HTTP {{ webhooks.find((item) => item.id === webhookForm.id)?.last_http_status }}
            </span>
            <span v-if="webhooks.find((item) => item.id === webhookForm.id)?.last_delivery_at">
              · {{ webhooks.find((item) => item.id === webhookForm.id)?.last_delivery_at }}
            </span>
            <span v-if="webhooks.find((item) => item.id === webhookForm.id)?.last_error">
              · {{ webhooks.find((item) => item.id === webhookForm.id)?.last_error }}
            </span>
          </div>

          <footer class="actions">
            <SubmitButton
              :block="false"
              :disabled="!hasWebhookChanges || webhooksSaving"
              :loading="webhooksSaving"
              prepend-icon="mdi-content-save"
              title="Save webhook">
              Save
            </SubmitButton>
            <span class="actions__spacer" />
            <v-btn
              v-if="webhookForm.id"
              variant="text"
              color="error"
              :disabled="webhooksSaving || webhooksDeletingId === webhookForm.id"
              @click="deleteWebhook(webhookForm.id)">
              Delete
            </v-btn>
            <v-btn
              variant="outlined"
              color="primary"
              :disabled="webhooksSaving"
              @click="resetWebhookSettings">
              Reset
            </v-btn>
          </footer>
        </form>
      </div>
    </section>
  </main>
</template>

<style scoped lang="scss">
@use "./systemSettings.scss";

.webhookSection {
  display: grid;
  grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
  gap: 1.5rem;
}

.webhookList {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.webhookListItem {
  text-align: left;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding: 0.9rem 1rem;
  border-radius: var(--nr-radius-md);
  border: 1px solid var(--nr-border-color);
  background: var(--nr-surface);
  color: var(--nr-text-primary);
}

.webhookListItem--active {
  border-color: var(--nr-input-border-hover);
  box-shadow: var(--nr-focus-ring);
}

.webhookListItem__name {
  font-weight: 600;
}

.webhookListItem__meta,
.webhookStatus {
  color: var(--nr-text-secondary, rgba(0, 0, 0, 0.6));
  font-size: 0.92rem;
}

.webhookForm {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.eventPicker {
  display: flex;
  gap: 1rem;
  flex-wrap: wrap;
}

.eventPicker__option {
  display: inline-flex;
  gap: 0.5rem;
  align-items: center;
}

@media (max-width: 720px) {
  .webhookSection {
    grid-template-columns: 1fr;
  }
}
</style>
