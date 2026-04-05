<template>
  <v-dialog
    v-model="dialogVisible"
    max-width="420"
    persistent
    scrim="true">
    <v-card>
      <v-card-title class="d-flex align-center gap-3">
        <v-icon color="error" size="24">mdi-alert-circle</v-icon>
        <span class="text-h6">{{ title }}</span>
      </v-card-title>

      <v-card-text>
        <div class="text-body-1 whitespace-pre-wrap">{{ message }}</div>
      </v-card-text>

      <v-card-actions class="justify-end pa-4">
        <v-btn
          color="primary"
          variant="text"
          @click="$emit('close')"
          class="text-none">
          Dismiss
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed } from 'vue';

const props = defineProps({
  visible: {
    type: Boolean,
    default: false,
  },
  title: {
    type: String,
    default: "",
  },
  message: {
    type: String,
    default: "",
  },
});

defineEmits<{
  (e: "close"): void;
}>();

const dialogVisible = computed({
  get: () => props.visible,
  set: (value) => {
    if (!value) {
      // Emit close when the dialog is being closed
      // This handles both programmatic closing and user interaction
    }
  }
});
</script>

<style scoped lang="scss">
.whitespace-pre-wrap {
  white-space: pre-wrap;
  line-height: 1.5;
}
</style>
