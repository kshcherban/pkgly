<template>
  <BrowseFile
    v-if="props.file.type === 'File'"
    :file="props.file.value as RawFile"
    :currentPath="props.currentPath"
    :repository="props.repository"
    :selected="props.selected"
    :selectable="props.selectable"
    @toggle-select="emit('toggle-select', $event)" />
  <BrowseFolder
    v-else-if="props.file.type === 'Directory'"
    :file="props.file.value as RawDirectory"
    :currentPath="props.currentPath"
    :repository="props.repository" />
</template>

<script setup lang="ts">
import { type RawBrowseFile, type RawDirectory, type RawFile } from "@/types/browse";
import { type RepositoryWithStorageName } from "@/types/repository";
import { type PropType } from "vue";
import BrowseFolder from "./BrowseFolder.vue";
import BrowseFile from "./BrowseFile.vue";

const props = defineProps({
  file: {
    type: Object as PropType<RawBrowseFile>,
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
</script>
