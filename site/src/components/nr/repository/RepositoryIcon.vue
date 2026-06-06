<!-- ABOUTME: Renders a repository ecosystem icon linked to its external homepage. -->
<!-- ABOUTME: Keeps external icon links labeled and isolated from the current page. -->
<template>
  <a
    target="_blank"
    rel="noopener noreferrer"
    :title="icon.name"
    :href="icon.url">
    <component
      :is="icon.component"
      v-bind="icon.props"
  /></a>
</template>
<script setup lang="ts">
import type { RepositoryIconDef } from "@/types/repository";
import type { PropType } from "vue";
defineProps({
  icon: {
    type: Object as PropType<RepositoryIconDef>,
    required: true,
  },
});
</script>

<style lang="scss" scoped>
a {
  display: inline-block;
  margin: 0.5rem;
}
</style>
