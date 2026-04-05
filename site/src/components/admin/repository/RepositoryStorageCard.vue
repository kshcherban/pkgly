<template>
  <section class="storage-card" data-testid="storage-card">
    <header class="storage-card__header">
      <div>
        <p class="storage-card__eyebrow">Storage</p>
        <h3 class="storage-card__title">
          {{ storageNameLabel }}
        </h3>
      </div>
      <span class="storage-card__status" :data-active="storage?.active ?? false">
        {{ (storage?.active ?? false) ? "Active" : "Inactive" }}
      </span>
    </header>

    <div v-if="loading" class="storage-card__body">Loading storage settings…</div>
    <div v-else-if="error" class="storage-card__body storage-card__error">{{ error }}</div>
    <div v-else-if="storage" class="storage-card__body">
      <dl class="storage-card__meta">
        <div class="storage-card__row">
          <dt>Type</dt>
          <dd>{{ storage.storage_type }}</dd>
        </div>
        <div class="storage-card__row">
          <dt>Identifier</dt>
          <dd>{{ storage.id }}</dd>
        </div>
      </dl>

      <div
        v-if="cache"
        class="storage-card__panel"
        data-testid="s3-cache-panel">
        <div class="storage-card__panel-title">S3 Local Cache</div>
        <p class="storage-card__panel-copy">
          Cached objects stay on disk for faster pulls before reading from the S3 bucket.
        </p>
        <dl class="storage-card__meta">
          <div class="storage-card__row">
            <dt>Status</dt>
            <dd>{{ cache.enabled ? "Enabled" : "Disabled" }}</dd>
          </div>
          <div class="storage-card__row">
            <dt>Directory</dt>
            <dd>{{ cache.path ?? defaultCachePath }}</dd>
          </div>
          <div class="storage-card__row">
            <dt>Max Size</dt>
            <dd>{{ formatBytes(cache.max_bytes) }}</dd>
          </div>
          <div class="storage-card__row">
            <dt>Max Entries</dt>
            <dd>{{ cache.max_entries }} entries</dd>
          </div>
        </dl>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import http from "@/http";
import type {
  S3CacheSettings,
  S3StorageSettings,
  StorageItem,
  StorageTypeConfig,
} from "@/components/nr/storage/storageTypes";
import { computed, onMounted, ref, watch } from "vue";

const props = defineProps<{
  storageId: string;
  storageName?: string;
}>();

const storage = ref<StorageItem | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);

const storageNameLabel = computed(() => {
  return props.storageName ?? storage.value?.name ?? "Storage";
});

const cache = computed<S3CacheSettings | null>(() => {
  if (!storage.value) {
    return null;
  }
  const config = storage.value.config as StorageTypeConfig | undefined;
  if (!config || (config as StorageTypeConfig).type?.toLowerCase() !== "s3") {
    return null;
  }
  const settings = (config as Extract<StorageTypeConfig, { type: "S3" }>).settings as S3StorageSettings;
  return settings.cache;
});

const defaultCachePath = computed(() => "/tmp/pkgly/s3-cache");

function formatBytes(bytes?: number | null): string {
  if (bytes === null || bytes === undefined) {
    return "Unknown";
  }
  if (bytes === 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  return `${value.toFixed(exponent === 0 ? 0 : 2)} ${units[exponent]}`;
}

async function loadStorage() {
  if (!props.storageId) {
    return;
  }
  loading.value = true;
  error.value = null;
  try {
    const response = await http.get(`/api/storage/${props.storageId}`);
    storage.value = response.data as StorageItem;
  } catch (err) {
    console.error("Failed to load storage", err);
    error.value = "Unable to load storage configuration.";
  } finally {
    loading.value = false;
  }
}

onMounted(loadStorage);
watch(
  () => props.storageId,
  () => {
    void loadStorage();
  },
);
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.storage-card {
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  padding: 1rem 1.25rem;
  background: var(--nr-surface, #fff);
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.storage-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.storage-card__eyebrow {
  font-size: 0.75rem;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: $text-50;
  margin: 0;
}

.storage-card__title {
  margin: 0;
  font-size: 1.1rem;
  font-weight: 600;
}

.storage-card__status {
  padding: 0.2rem 0.6rem;
  border-radius: 999px;
  font-size: 0.85rem;
  background: rgba(46, 125, 50, 0.12);
  color: #2e7d32;

  &[data-active="false"] {
    background: rgba(245, 124, 0, 0.14);
    color: #f57c00;
  }
}

.storage-card__body {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  font-size: 0.95rem;
}

.storage-card__error {
  color: #c62828;
}

.storage-card__meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 0.5rem 1rem;
}

.storage-card__row {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.06));
  border-radius: 10px;

  dt {
    margin: 0;
    font-weight: 600;
    color: $text-50;
  }

  dd {
    margin: 0;
    text-align: right;
    color: $text;
    word-break: break-word;
  }
}

.storage-card__panel {
  padding: 0.75rem 0.9rem;
  border-radius: 10px;
  background: rgba($primary, 0.04);
  border: 1px dashed rgba($primary, 0.35);
}

.storage-card__panel-title {
  font-weight: 600;
  margin-bottom: 0.15rem;
}

.storage-card__panel-copy {
  margin: 0 0 0.5rem 0;
  color: $text-50;
  font-size: 0.9rem;
}
</style>
