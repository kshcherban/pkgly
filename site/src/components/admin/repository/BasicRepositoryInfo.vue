<template>
  <v-card
    v-if="repository"
    data-testid="repository-info-card"
    :class="['repository-info-card', { 'repository-info-card--embedded': embedded }]"
    :variant="embedded ? 'flat' : undefined">
    <v-card-title class="repository-info-card__header">
      <div>
        <div class="text-h6">Repository Info</div>
        <div class="text-body-2 text-medium-emphasis">
          Operational details and current usage metrics.
        </div>
      </div>
      <v-chip
        size="small"
        class="text-uppercase font-weight-medium"
        :color="statusChip.color"
        variant="tonal"
        data-testid="repository-status-chip">
        {{ statusChip.label }}
      </v-chip>
    </v-card-title>

    <v-card-text>
      <v-row
        class="repository-info-card__grid"
        dense
        data-testid="repository-meta-grid">
        <v-col
          v-for="item in metaItems"
          :key="item.label"
          cols="12"
          md="6"
          lg="4">
          <div
            class="meta-tile"
            data-testid="repository-meta-item">
            <span class="meta-tile__label">{{ item.label }}</span>
            <span class="meta-tile__value">{{ item.value }}</span>
          </div>
        </v-col>
      </v-row>

      <v-divider class="my-6" />

      <div class="repository-info-card__actions">
        <div class="repository-info-card__auth">
          <span class="text-subtitle-2">Repository Authentication</span>
          <span class="text-body-2 text-medium-emphasis">
            {{ repository.auth_enabled ? "Enabled" : "Disabled" }}
          </span>
        </div>
        <div class="repository-info-card__buttons">
          <v-btn
            :color="toggleButton.color"
            variant="tonal"
            class="text-none"
            data-testid="repository-toggle"
            disabled>
            <v-icon
              class="mr-2"
              icon="mdi-toggle-switch" />
            {{ toggleButton.label }}
          </v-btn>
        <v-btn
          color="error"
          variant="flat"
          class="text-none danger-hover"
          data-testid="repository-delete"
          @click="openDeleteDialog">
            <v-icon
              class="mr-2"
              icon="mdi-delete-outline" />
            Delete Repository
          </v-btn>
        </div>
        <div
          class="repository-info-card__hint text-body-2 text-medium-emphasis"
          data-testid="repository-toggle-hint">
          Repository activation controls are coming soon.
        </div>
      </div>
    </v-card-text>
  </v-card>

  <v-dialog
    v-model="isDeleteDialogOpen"
    max-width="500"
    data-testid="repository-delete-dialog">
    <v-card>
      <v-card-title class="text-h6">
        Delete repository "{{ repository?.name ?? "this repository" }}"?
      </v-card-title>
      <v-card-text>
        <p class="mb-2">This will permanently remove all packages and metadata stored on its backing storage.</p>
        <p class="mb-0 font-weight-medium">This action cannot be undone.</p>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn
          variant="text"
          class="text-none"
          data-testid="repository-delete-cancel"
          @click="closeDeleteDialog">
          Cancel
        </v-btn>
        <v-btn
          color="error"
          variant="flat"
          class="text-none"
          :loading="isDeleting"
          data-testid="repository-delete-confirm"
          @click="confirmDelete">
          Delete
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>
<script setup lang="ts">
import http from "@/http";
import router from "@/router";
import type { RepositoryWithStorageName } from "@/types/repository";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref, type PropType } from "vue";

const props = defineProps({
  repository: {
    type: Object as PropType<RepositoryWithStorageName>,
    required: true,
  },
  embedded: {
    type: Boolean,
    default: false,
  },
});

const statusChip = computed(() => {
  if (!props.repository) {
    return { label: "Unavailable", color: "warning" as const };
  }
  return props.repository.active
    ? { label: "Active", color: "success" as const }
    : { label: "Inactive", color: "warning" as const };
});

const toggleButton = computed(() => {
  if (!props.repository || props.repository.active) {
    return { label: "Disable Repository", color: "warning" as const };
  }
  return { label: "Enable Repository", color: "primary" as const };
});

function repositoryTypeLabel(repo: RepositoryWithStorageName) {
  const kind = (repo.repository_kind ?? "hosted").toLowerCase();
  return `${repo.repository_type.toLowerCase()} (${kind})`;
}

const metaItems = computed(() => {
  if (!props.repository) {
    return [];
  }
  return [
    {
      label: "Repository Name",
      value: props.repository.name,
    },
    {
      label: "Repository Type",
      value: repositoryTypeLabel(props.repository),
    },
    {
      label: "Storage Name",
      value: props.repository.storage_name,
    },
    {
      label: "Storage Identifier",
      value: props.repository.storage_id,
    },
    {
      label: "Storage Usage",
      value: formatBytes(props.repository.storage_usage_bytes),
    },
    {
      label: "Usage Updated",
      value: formatUpdatedAt(props.repository.storage_usage_updated_at),
    },
  ];
});

function formatBytes(bytes?: number | null): string {
  if (bytes === null || bytes === undefined) {
    return "Unknown";
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
    return "Unknown";
  }
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return "Unknown";
  }
  return date.toLocaleString();
}
const alerts = useAlertsStore();
const isDeleteDialogOpen = ref(false);
const isDeleting = ref(false);

function openDeleteDialog() {
  isDeleteDialogOpen.value = true;
}

function closeDeleteDialog() {
  isDeleteDialogOpen.value = false;
}

async function confirmDelete() {
  if (!props.repository) {
    return;
  }

  isDeleting.value = true;
  try {
    await http.delete(`/api/repository/${props.repository.id}`);
    alerts.success("Repository deleted", "Repository has been deleted.");
    closeDeleteDialog();
    router.push({ name: "RepositoriesList" });
  } catch (error) {
    console.error(error);
    alerts.error("Failed to delete repository", "An error occurred while deleting repository.");
  } finally {
    isDeleting.value = false;
  }
}

</script>
<style lang="scss" scoped>
@use "@/assets/styles/theme.scss" as *;

.repository-info-card {
  &--embedded {
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  &__header {
    align-items: flex-start;
    gap: 1rem;
  }

  &__grid {
    row-gap: 1rem;
  }

  &__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 1.5rem;
  }

  &__auth {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  &__hint {
    flex-basis: 100%;
    margin-top: -0.5rem;
  }

  &__buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    justify-content: flex-end;
  }
}

.meta-tile {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  padding: 0.75rem 1rem;
  background-color: rgba($primary, 0.07);
  border-radius: 12px;

  &__label {
    font-size: 0.8125rem;
    letter-spacing: 0.02em;
    font-weight: 600;
    color: $text-50;
    text-transform: uppercase;
  }

  &__value {
    font-size: 1rem;
    color: $text;
    word-break: break-word;
  }
}

@media (max-width: 960px) {
  .repository-info-card__header {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
