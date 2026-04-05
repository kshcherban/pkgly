<template>
  <tr
    class="browse__row browse__row--directory"
    data-type="folder"
    role="button"
    tabindex="0"
    @click="activate"
    @keyup.enter.prevent="activate"
    @keyup.space.prevent="activate">
    <td class="browse__cell browse__cell--select"></td>
    <td class="browse__cell browse__cell--name">
      <div class="browse__cell-content">
        <font-awesome-icon icon="fa-solid fa-folder" />
        <span class="browse__name">{{ props.file.name }}</span>
      </div>
    </td>
    <td class="browse__cell browse__cell--meta">
      {{ directorySummary }}
    </td>
  </tr>
</template>

<script setup lang="ts">
import router from "@/router";
import { fixCurrentPath, type RawDirectory } from "@/types/browse";
import { type RepositoryWithStorageName } from "@/types/repository";
import { computed, type PropType } from "vue";
import "./browse.scss";

const props = defineProps({
  file: {
    type: Object as PropType<RawDirectory>,
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
const fixedPath = fixCurrentPath(props.currentPath);
const browseRoute = `/browse/${props.repository.id}/${fixedPath}/${props.file.name}`;

const directorySummary = computed(() => {
  const count = props.file.number_of_files;
  if (typeof count !== "number") {
    return "";
  }
  return `${count} item${count === 1 ? "" : "s"}`;
});

function activate() {
  router.push(browseRoute);
}
</script>
