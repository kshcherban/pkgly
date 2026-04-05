<template>
  <div id="browseList">
    <div class="browse__actions">
      <label class="browse__bulk-toggle">
        <input
          type="checkbox"
          data-testid="browse-select-all-files"
          :checked="allFilesSelected"
          :disabled="selectableFiles.length === 0"
          @change="toggleAllFiles(($event.target as HTMLInputElement).checked)" />
        <span>Select all files</span>
      </label>
      <button
        type="button"
        class="browse__download-button"
        data-testid="browse-download-selected"
        :disabled="selectedCount === 0"
        @click="downloadSelected">
        Download Selected
      </button>
    </div>
    <table class="browse__table">
      <thead>
        <tr>
          <th
            class="browse__header-cell browse__header-cell--select"
            data-column="select"></th>
          <th
            class="browse__header-cell browse__header-cell--name"
            data-column="name">
            Name
          </th>
          <th
            class="browse__header-cell browse__header-cell--meta"
            data-column="details">
            Details
          </th>
        </tr>
      </thead>
      <tbody>
        <BrowseEntry
          v-for="file in sortedFiles"
          :key="file.value.name"
          :file="file"
          :currentPath="currentPath"
          :repository="repository"
          :selected="file.type === 'File' ? isSelected(file.value.name) : false"
          :selectable="file.type === 'File' ? isSelectableFile(file.value.name) : false"
          @toggle-select="toggleSelection(file, $event)" />
        <SkeletonEntry
          v-for="i in skeletons"
          :key="i" />
      </tbody>
    </table>
  </div>
</template>
<script setup lang="ts">
import { fixCurrentPath, type RawBrowseFile } from "@/types/browse";
import { type RepositoryWithStorageName } from "@/types/repository";
import { createBrowseFileRoute } from "@/types/repositoryRoute";

import BrowseEntry from "./BrowseEntry.vue";
import { computed, nextTick, ref, type PropType, watch } from "vue";
import SkeletonEntry from "./SkeletonEntry.vue";
import { useResizableColumns } from "@/composables/useResizableColumns";

const props = defineProps({
  files: {
    type: Array as PropType<RawBrowseFile[]>,
    required: true,
  },
  totalFiles: {
    type: Number,
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
});
const skeletons = computed(() => {
  const skeletonsArray = [];
  for (let i = props.files.length; i < props.totalFiles; i++) {
    skeletonsArray.push(i);
  }
  return skeletonsArray;
});
const sortedFiles = computed(() => {
  const files = [...props.files];
  return files.sort((a, b) => {
    if (a.type === "Directory" && b.type === "File") {
      return -1;
    } else if (a.type === "File" && b.type === "Directory") {
      return 1;
    } else {
      return a.value.name.localeCompare(b.value.name);
    }
  });
});
const selectedFiles = ref<Set<string>>(new Set());
const fixedPath = computed(() => fixCurrentPath(props.currentPath));
const selectableFiles = computed(() =>
  sortedFiles.value.filter(
    (file): file is Extract<RawBrowseFile, { type: "File" }> =>
      file.type === "File" && isSelectableFile(file.value.name),
  ),
);
const allFilesSelected = computed(() => {
  return selectableFiles.value.length > 0 && selectedFiles.value.size === selectableFiles.value.length;
});
const selectedCount = computed(() => selectedFiles.value.size);

const { initResizable: initBrowseResizers } = useResizableColumns("#browseList .browse__table");

function filePath(name: string): string {
  if (fixedPath.value.length === 0) {
    return name;
  }
  return `${fixedPath.value}/${name}`;
}

function downloadUrlFor(name: string): string | null {
  return createBrowseFileRoute(props.repository, props.currentPath, name);
}

function isSelectableFile(name: string): boolean {
  return downloadUrlFor(name) !== null;
}

function isSelected(name: string): boolean {
  return selectedFiles.value.has(filePath(name));
}

function toggleSelection(file: RawBrowseFile, checked: boolean) {
  if (file.type !== "File" || !isSelectableFile(file.value.name)) {
    return;
  }
  const next = new Set(selectedFiles.value);
  const path = filePath(file.value.name);
  if (checked) {
    next.add(path);
  } else {
    next.delete(path);
  }
  selectedFiles.value = next;
}

function toggleAllFiles(checked: boolean) {
  if (!checked) {
    selectedFiles.value = new Set();
    return;
  }
  selectedFiles.value = new Set(selectableFiles.value.map((file) => filePath(file.value.name)));
}

function downloadSelected() {
  for (const file of selectableFiles.value) {
    const path = filePath(file.value.name);
    if (!selectedFiles.value.has(path)) {
      continue;
    }
    const downloadUrl = downloadUrlFor(file.value.name);
    if (downloadUrl === null) {
      continue;
    }
    const link = document.createElement("a");
    link.href = downloadUrl;
    link.download = file.value.name;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  }
}

watch(
  () => sortedFiles.value.length,
  (length) => {
    if (length === 0) {
      return;
    }
    nextTick(() => {
      initBrowseResizers();
    });
  },
  { flush: "post" },
);

watch(
  () => [props.currentPath, props.files] as const,
  () => {
    selectedFiles.value = new Set();
  },
);
</script>
<style lang="scss" scoped>
@use "@/assets/styles/theme.scss" as *;
#browseList {
  padding: 1rem;
}

.browse__actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  margin-bottom: 0.75rem;
}

.browse__bulk-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
}

.browse__download-button {
  padding: 0.45rem 0.85rem;
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
}

.browse__download-button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.browse__table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  background: var(--nr-background-primary, #fff);
  border: 1px solid rgba(0, 0, 0, 0.08);
  border-radius: 8px;
  overflow: hidden;
}

.browse__header-cell {
  text-align: left;
  font-weight: 600;
  font-size: 0.9rem;
  padding: 0.75rem 1rem;
  background: var(--nr-background-tertiary, #f8f9fa);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  position: relative;
  overflow: visible;
  padding-right: 1.25rem;
}

.browse__header-cell--meta {
  text-align: right;
}

.browse__header-cell--select {
  width: 3rem;
}
</style>
