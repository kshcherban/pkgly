<template>
  <div class="subBarParent">
    <slot name="button"></slot>
    <div
      class="subBar"
      :data-is-open="isOpen">
      <slot name="content"></slot>
    </div>
  </div>
</template>
<script setup lang="ts">
import router from "@/router";
import { computed, defineProps } from "vue";
const props = defineProps({
  isOpen: {
    type: Boolean,
    required: false,
  },
  openIfHasTag: {
    type: String,
  },
});
const isOpen = computed(() => {
  if (props.openIfHasTag) {
    return router.currentRoute.value.meta.tag === props.openIfHasTag;
  }
  if (props.isOpen !== undefined) {
    return props.isOpen;
  }
  console.error("No isOpen or openIfHasTag provided");
  return false;
});
</script>
<style scoped lang="scss">
.subBarParent {
  .subBar {
    padding-left: 1rem;
  }
}
@media (prefers-reduced-motion: no-preference) {
  .subBar {
    transition: max-height 0.2s ease, opacity 0.2s ease;
  }
}
</style>
