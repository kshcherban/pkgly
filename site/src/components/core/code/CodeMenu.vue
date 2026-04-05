<template>
  <v-card
    variant="flat"
    class="code-menu">
    <v-tabs
      v-model="activeTab"
      density="compact"
      class="code-menu__tabs">
      <v-tab
        v-for="snippet in snippets"
        :key="snippet.key"
        :value="snippet.key">
        {{ snippet.name }}
      </v-tab>
    </v-tabs>

    <v-window
      v-model="activeTab"
      class="code-menu__window">
      <v-window-item
        v-for="snippet in snippets"
        :key="snippet.key"
        :value="snippet.key">
        <CodeCard :code="snippet" />
      </v-window-item>
    </v-window>
  </v-card>
</template>

<script setup lang="ts">
import { ref, watch, type PropType } from "vue";
import type { CodeSnippet } from "./code";

import CodeCard from "./CodeCard.vue";

const props = defineProps({
  snippets: {
    type: Array as PropType<CodeSnippet[]>,
    required: true,
  },
  defaultTab: {
    type: String,
    required: true,
  },
});

const activeTab = ref(props.defaultTab);

watch(
  () => props.snippets,
  (snippets) => {
    if (snippets.length === 0) {
      activeTab.value = "";
      return;
    }
    if (!snippets.some((snippet) => snippet.key === activeTab.value)) {
      activeTab.value = snippets[0]?.key ?? "";
    }
  },
  { immediate: true },
);
</script>

<style scoped lang="scss">
.code-menu__tabs {
  background-color: var(--v-theme-surface);
}

.code-menu__window {
  padding-top: 0.5rem;
}
</style>
