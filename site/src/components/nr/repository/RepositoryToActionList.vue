<template>
  <div
    v-auto-animate
    id="repositoryEntries">
      <div
      id="header"
      class="row">
      <div class="col">Repository</div>
      <div
        class="col"
        aria-hidden="true"></div>
      <div
        class="col"
        aria-hidden="true"></div>
      <div
        class="col"
        aria-hidden="true"></div>
      <div class="col action">Action</div>
    </div>
    <div
      class="row item"
      v-for="entry in repositoryEntries"
      :key="entry.repositoryId">
      <div class="col">{{ getRepositoryName(entry.repositoryId) }}</div>
      <div class="col repository-switch">
        <SwitchInput
          :id="`repo-${entry.repositoryId}-read`"
          v-model="entry.actions.can_read">
          Read
        </SwitchInput>
      </div>
      <div class="col repository-switch">
        <SwitchInput
          :id="`repo-${entry.repositoryId}-write`"
          v-model="entry.actions.can_write">
          Write
        </SwitchInput>
      </div>
      <div class="col repository-switch">
        <SwitchInput
          :id="`repo-${entry.repositoryId}-edit`"
          v-model="entry.actions.can_edit">
          Edit
        </SwitchInput>
      </div>
      <div class="col">
        <button
          type="button"
          class="actionButton actionButton--danger actionButton--fixed-width"
          @click="removeEntry(entry.repositoryId)">
          Remove
        </button>
      </div>
    </div>
    <div
      class="row item"
      id="create">
      <div
        class="col"
        id="repoDropDown">
        <RepositoryDropdown v-model="newEntry.repositoryId" />
      </div>
      <div class="col repository-switch">
        <SwitchInput
          id="new-repo-read"
          v-model="newEntry.actions.can_read">
          Read
        </SwitchInput>
      </div>
      <div class="col repository-switch">
        <SwitchInput
          id="new-repo-write"
          v-model="newEntry.actions.can_write">
          Write
        </SwitchInput>
      </div>
      <div class="col repository-switch">
        <SwitchInput
          id="new-repo-edit"
          v-model="newEntry.actions.can_edit">
          Edit
        </SwitchInput>
      </div>
      <div class="col">
        <button
          type="button"
          class="actionButton actionButton--primary actionButton--fixed-width"
          @click="addEntry"
          :disabled="!isNewEntryValid">
          Add
        </button>
      </div>
    </div>
  </div>
</template>
<script setup lang="ts">
import SwitchInput from "@/components/form/SwitchInput.vue";
import RepositoryDropdown from "@/components/form/dropdown/RepositoryDropdown.vue";
import { useRepositoryStore } from "@/stores/repositories";
import { RepositoryActionsType } from "@/types/user";
import { type NewAuthTokenRepositoryScope } from "@/types/user/token";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref } from "vue";

const repositoryStore = useRepositoryStore();
function getRepositoryName(repositoryId: string) {
  const repository = repositoryStore.getRepositoryFromCache(repositoryId);
  return repository ? repository.name : repositoryId;
}
const repositoryEntries = defineModel<Array<NewAuthTokenRepositoryScope>>({
  required: true,
});
const newEntry = ref<NewAuthTokenRepositoryScope>({
  repositoryId: "",
  actions: new RepositoryActionsType([]),
});
const alerts = useAlertsStore();
const isNewEntryValid = computed(() => {
  if (!newEntry.value.repositoryId || newEntry.value.repositoryId == "") {
    return false;
  }

  if (newEntry.value.actions.asArray().length === 0) {
    return false;
  }
  return true;
});

function addEntry() {
  for (const repository of repositoryEntries.value) {
    if (repository.repositoryId === newEntry.value.repositoryId) {
      repository.actions = newEntry.value.actions;
      alerts.success("Repository already exists", "Values have been updated.");
      newEntry.value = {
        repositoryId: "",
        actions: new RepositoryActionsType([]),
      };
      return;
    }
  }
  repositoryEntries.value.push({
    repositoryId: newEntry.value.repositoryId,
    actions: newEntry.value.actions,
  });
  newEntry.value = {
    repositoryId: "",
    actions: new RepositoryActionsType([]),
  };
}

function removeEntry(id: string) {
  repositoryEntries.value = repositoryEntries.value.filter((entry) => entry.repositoryId !== id);
}
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

#repositoryEntries {
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr 1fr 1fr;
  }
  .repository-switch {
    display: flex;
    align-items: center;
    justify-content: center;

    :deep(.switch-wrapper) {
      margin: 0;
    }
  }
  .actionButton {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    padding: 0.5rem 1.25rem;
    border-radius: 0.5rem;
    cursor: pointer;
    font-weight: 500;
    transition: background-color 0.2s ease;
  }

  .actionButton--fixed-width {
    min-width: 6.25rem;
  }

  .actionButton--primary {
    background-color: $primary;
    color: white;

    &:disabled {
      background-color: $primary-50;
      cursor: not-allowed;
    }
  }

  .actionButton--danger {
    background-color: var(--nr-error);
    color: white;

    &:hover {
      background-color: var(--nr-error-dark);
    }

    &:disabled {
      background-color: var(--nr-error-light);
      cursor: not-allowed;
    }
  }
  #header {
    border-bottom: 1px solid $primary-50;
    padding: 1rem 0rem;
    .col {
      font-weight: bold;
    }
  }
  .row {
    padding: 1rem;
    padding-top: 0.5rem;
  }
}
</style>
