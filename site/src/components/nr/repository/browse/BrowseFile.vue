<template>
  <tr
    class="browse__row browse__row--file"
    :data-testid="`browse-row-file-${props.file.name}`"
    data-type="file"
    role="button"
    tabindex="0"
    @click="activate"
    @keyup.enter.prevent="activate"
    @keyup.space.prevent="activate">
    <td class="browse__cell browse__cell--select">
      <input
        type="checkbox"
        :checked="props.selected"
        :disabled="!props.selectable"
        :data-testid="`browse-select-file-${props.file.name}`"
        @click.stop
        @change="toggleSelection(($event.target as HTMLInputElement).checked)" />
    </td>
    <td class="browse__cell browse__cell--name">
      <div class="browse__cell-content">
        <font-awesome-icon :icon="fileIcon" />
        <span class="browse__name">{{ props.file.name }}</span>
      </div>
    </td>
    <td class="browse__cell browse__cell--meta">
      {{ formattedModified }}
    </td>
  </tr>
</template>

<script setup lang="ts">
import { type RawFile } from "@/types/browse";
import { type RepositoryWithStorageName } from "@/types/repository";
import { createBrowseFileRoute } from "@/types/repositoryRoute";
import { computed, type PropType } from "vue";
import "./browse.scss";
const props = defineProps({
  file: {
    type: Object as PropType<RawFile>,
    required: true,
  },
  currentPath: {
    type: String,
    required: true,
  },
  repository: {
    type: Object as PropType<RepositoryWithStorageName>,
    required: true,
  },
  selected: {
    type: Boolean,
    default: false,
  },
  selectable: {
    type: Boolean,
    default: true,
  },
});
const emit = defineEmits<{
  "toggle-select": [checked: boolean];
}>();

const repositoryURL = computed(() =>
  createBrowseFileRoute(props.repository, props.currentPath, props.file.name),
);

const fileIcon = computed(() => "fa-solid fa-file" /* TODO: file-type specific */);

const formattedModified = computed(() =>
  new Date(props.file.modified).toLocaleString(),
);

function activate() {
  if (repositoryURL.value === null) {
    return;
  }
  window.open(repositoryURL.value, "_blank");
}

function toggleSelection(checked: boolean) {
  if (!props.selectable) {
    return;
  }
  emit("toggle-select", checked);
}
</script>
