<template>
  <RouterLink
    :to="to"
    :data-active="isActive"
    class="navLink">
    <slot />
  </RouterLink>
</template>
<script setup lang="ts">
import { computed, defineProps } from "vue";
import { useRouter } from "vue-router";
const props = defineProps({
  to: {
    type: String,
    required: true,
  },
  routeName: {
    type: String,
    required: false,
  },
});
const router = useRouter();
const isActive = computed(() => {
  return props.routeName === router.currentRoute.value.name;
});
</script>
<style scoped lang="scss">
.navLink {
  text-decoration: none;
  color: var(--nr-text-primary);
  font-weight: 500;
  padding: 0.5rem;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  border-radius: 0.5rem;
  transition: background-color 0.2s ease;

  &:hover {
    background-color: var(--nr-table-row-hover);
    color: var(--nr-primary);
  }
}

.navLink[data-active="true"] {
  background-color: rgba(30, 136, 229, 0.12);
  color: var(--nr-primary);

  &:hover {
    cursor: default;
  }
}
</style>
