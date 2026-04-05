<template>
  <div
    v-if="alerts.length"
    class="global-alerts">
    <transition-group name="global-alert" tag="div">
      <v-alert
        v-for="alert in alerts"
        :key="alert.id"
        :type="alert.kind"
        variant="tonal"
        border="start"
        density="comfortable"
        class="global-alert"
        :data-kind="alert.kind"
        closable
        @click:close="dismiss(alert.id)">
        <div class="global-alert__title">{{ alert.title }}</div>
        <div v-if="alert.message" class="global-alert__message">{{ alert.message }}</div>
      </v-alert>
    </transition-group>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import { useAlertsStore } from "@/stores/alerts";

const alertsStore = useAlertsStore();
const { alerts } = storeToRefs(alertsStore);

function dismiss(id: number) {
  alertsStore.dismiss(id);
}
</script>

<style scoped lang="scss">
.global-alerts {
  position: fixed;
  right: 1.5rem;
  bottom: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  width: min(380px, calc(100vw - 2rem));
  z-index: 2200;
}

.global-alert {
  box-shadow: var(--nr-shadow-8);
}

.global-alert[data-kind="warning"] {
  border-left-color: var(--nr-warning);
}

.global-alert[data-kind="info"] {
  border-left-color: var(--nr-info);
}

.global-alert__title {
  font-weight: 600;
  margin-bottom: 0.25rem;
}

.global-alert__message {
  font-size: 0.95rem;
}

.global-alert-enter-from,
.global-alert-leave-to {
  opacity: 0;
  transform: translateX(12px);
}

.global-alert-enter-active,
.global-alert-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.global-alert-leave-active {
  position: absolute;
}

@media (max-width: 600px) {
  .global-alerts {
    left: 1rem;
    right: 1rem;
    bottom: 1rem;
    width: auto;
  }
}
</style>
