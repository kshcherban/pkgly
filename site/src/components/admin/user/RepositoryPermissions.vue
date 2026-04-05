<template>
  <section class="repository-permissions">
    <header class="repository-permissions__header">
      <div>
        <h2 class="text-h5 mb-1">Repository Permissions</h2>
        <p class="text-body-2 text-medium-emphasis">
          Manage explicit repository access overrides for this user.
        </p>
      </div>
    </header>

    <div
      v-auto-animate
      class="repository-permissions__table">
      <div class="repository-permissions__row repository-permissions__row--header">
        <div>Repository</div>
        <div class="repository-permissions__toggle-heading" aria-hidden="true"></div>
        <div class="repository-permissions__toggle-heading" aria-hidden="true"></div>
        <div class="repository-permissions__toggle-heading" aria-hidden="true"></div>
        <div class="repository-permissions__actions-heading">Action</div>
      </div>

      <div
        v-for="repository in repositoryPermissions"
        :key="repository.id"
        class="repository-permissions__row">
        <div class="repository-permissions__name">{{ repository.name }}</div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            :id="`repo-${repository.id}-read`"
            v-model="repository.permissions.can_read"
            class="repository-permissions__switch">
            Read
          </SwitchInput>
        </div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            :id="`repo-${repository.id}-write`"
            v-model="repository.permissions.can_write"
            class="repository-permissions__switch">
            Write
          </SwitchInput>
        </div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            :id="`repo-${repository.id}-edit`"
            v-model="repository.permissions.can_edit"
            class="repository-permissions__switch">
            Edit
          </SwitchInput>
        </div>
        <div class="repository-permissions__row-actions">
          <v-btn
            variant="flat"
            color="error"
            prepend-icon="mdi-delete"
            class="repository-permissions__button repository-permissions__button--danger"
            @click="deleteRepository(repository.id)">
            Delete
          </v-btn>
        </div>
      </div>

      <div class="repository-permissions__row repository-permissions__row--create">
        <div class="repository-permissions__name repository-permissions__name--input">
          <RepositoryDropdown v-model="newEntry.repository" />
        </div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            id="new-repo-read"
            v-model="newEntry.actions.can_read"
            class="repository-permissions__switch">
            Read
          </SwitchInput>
        </div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            id="new-repo-write"
            v-model="newEntry.actions.can_write"
            class="repository-permissions__switch">
            Write
          </SwitchInput>
        </div>
        <div class="repository-permissions__toggle">
          <SwitchInput
            id="new-repo-edit"
            v-model="newEntry.actions.can_edit"
            class="repository-permissions__switch">
            Edit
          </SwitchInput>
        </div>
        <div class="repository-permissions__row-actions">
          <v-btn
            variant="flat"
            color="primary"
            prepend-icon="mdi-plus"
            class="repository-permissions__button repository-permissions__button--primary"
            @click="addRepository"
            :disabled="!isNewEntryValid">
            Add
          </v-btn>
        </div>
      </div>
    </div>

    <footer class="repository-permissions__footer">
      <SubmitButton
        variant="flat"
        :block="false"
        :disabled="!hasChanged"
        @click="save"
        prepend-icon="mdi-content-save">
        Save
      </SubmitButton>
    </footer>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch, type PropType } from "vue";
import type { RepositoryActions, UserResponseType } from "@/types/base";
import { useRepositoryStore } from "@/stores/repositories";
import RepositoryDropdown from "@/components/form/dropdown/RepositoryDropdown.vue";
import { useAlertsStore } from "@/stores/alerts";
import http from "@/http";
import { RepositoryActionsType, type FullPermissions } from "@/types/user";
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";

const props = defineProps({
  user: {
    type: Object as PropType<UserResponseType>,
    required: true,
  },
});

const originalPermissions = ref<FullPermissions | undefined>(undefined);
const repositoryPermissions = ref<
  {
    id: string;
    name: string;
    permissions: RepositoryActionsType;
  }[]
>([]);
const repoStore = useRepositoryStore();

const hasChanged = ref(false);

const isNewEntryValid = computed(() => {
  if (!newEntry.value.repository) {
    return false;
  }
  if (newEntry.value.repository.length === 0) {
    return false;
  }
  return true;
});

const newEntry = ref({
  repository: "",
  actions: new RepositoryActionsType([]),
});

const alerts = useAlertsStore();

function deleteRepository(repository: string) {
  for (let i = 0; i < repositoryPermissions.value.length; i++) {
    if (repositoryPermissions.value[i]?.id === repository) {
      repositoryPermissions.value.splice(i, 1);
      return;
    }
  }
}

async function addRepository() {
  if (!isNewEntryValid.value) {
    return;
  }
  for (const repository of repositoryPermissions.value) {
    if (repository.id === newEntry.value.repository) {
      repository.permissions.update(newEntry.value.actions);
      alerts.success("Repository already exists", "Permissions have been updated.");
      return;
    }
  }
  const repositoryValue = await repoStore.getRepositoryById(newEntry.value.repository);
  if (!repositoryValue) {
    alerts.error("Repository not found", "The repository could not be found.");
    return;
  }

  repositoryPermissions.value.push({
    id: newEntry.value.repository,
    name: repositoryValue.name,
    permissions: new RepositoryActionsType(newEntry.value.actions.asArray()),
  });

  newEntry.value.repository = "";
  newEntry.value.actions.can_read = false;
  newEntry.value.actions.can_write = false;
  newEntry.value.actions.can_edit = false;
}

async function loadUserPermissions() {
  await http
    .get<FullPermissions>(`api/user-management/get/${props.user.id}/permissions`)
    .then((response) => {
      originalPermissions.value = response.data;
    })
    .catch((error) => {
      alerts.error("Error loading permissions", "An error occurred while loading permissions.");
      console.error(error);
    });
}

async function load() {
  await loadUserPermissions();
  if (!originalPermissions.value) {
    console.error("No permissions found");
    return;
  }

  for (const [repository, actions] of Object.entries(
    originalPermissions.value.repository_permissions,
  )) {
    const repositoryValue = await repoStore.getRepositoryById(repository);
    if (!repositoryValue) {
      console.error(`Repository ${repository} not found`);
      continue;
    }
    repositoryPermissions.value.push({
      id: repository,
      name: repositoryValue.name,
      permissions: new RepositoryActionsType(actions),
    });
  }
}

load();

watch(
  repositoryPermissions,
  () => {
    if (!originalPermissions.value) {
      return;
    }
    if (
      repositoryPermissions.value.length !==
      Object.keys(originalPermissions.value.repository_permissions).length
    ) {
      hasChanged.value = true;
      return;
    }
    for (const repository of repositoryPermissions.value) {
      if (
        !originalPermissions.value.repository_permissions[repository.id] ||
        !repository.permissions.equalsArray(
          originalPermissions.value.repository_permissions[repository.id] as Array<RepositoryActions>,
        )
      ) {
        hasChanged.value = true;
        return;
      }
    }
    hasChanged.value = false;
  },
  { deep: true },
);

async function save() {
  const repositoryPermissionsValue: Record<string, Array<RepositoryActions>> = {};
  for (const repository of repositoryPermissions.value) {
    repositoryPermissionsValue[repository.id] = repository.permissions.asArray();
  }
  const newPermissions = {
    repository_permissions: repositoryPermissionsValue,
  };

  await http
    .put(`/api/user-management/update/${props.user.id}/permissions`, newPermissions)
    .then(() => {
      alerts.success("Permissions saved", "Permissions have been saved.");
    })
    .catch((error) => {
      let text = "An error occurred while saving permissions.";
      if (error.response?.data) {
        text = error.response.data;
      }
      alerts.error("Error saving permissions", text);
    });
}
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;

.repository-permissions {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.repository-permissions__header {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.repository-permissions__table {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--nr-border-muted);
  border-radius: 12px;
  overflow: hidden;
  background: var(--nr-surface);
}

.repository-permissions__row {
  display: grid;
  grid-template-columns: minmax(0, 2fr) repeat(3, minmax(0, 1fr)) auto;
  gap: 1rem;
  padding: 1rem 1.25rem;
  align-items: center;
}

.repository-permissions__row + .repository-permissions__row {
  border-top: 1px solid var(--nr-border-subtle);
}

.repository-permissions__row--header {
  background: var(--nr-surface-elevated);
  font-weight: 600;
  color: var(--nr-text-secondary);
}

.repository-permissions__row--create {
  background: var(--nr-surface-elevated);
}

.repository-permissions__name {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.repository-permissions__name--input {
  align-items: stretch;
  min-width: 0;
}

.repository-permissions__name--input :deep(.repository-dropdown),
.repository-permissions__name--input :deep(select) {
  width: 100%;
}

.repository-permissions__toggle {
  display: flex;
  justify-content: center;
}

.repository-permissions__toggle :deep(.switch-wrapper) {
  margin: 0;
  display: flex;
  justify-content: center;
}

.repository-permissions__toggle :deep(.v-switch) {
  margin: 0;
}

.repository-permissions__toggle-heading {
  text-align: center;
}

.repository-permissions__row-actions {
  display: flex;
  justify-content: flex-end;
}

.repository-permissions__button {
  min-width: 110px;
  text-transform: none;
  font-weight: 600;
  transition: filter 0.2s ease, box-shadow 0.2s ease;
}

.repository-permissions__button :deep(.v-btn__overlay) {
  background-color: transparent;
}

.repository-permissions__button--primary {
  color: var(--v-theme-on-primary);
}

.repository-permissions__button--danger {
  color: var(--v-theme-on-error);
}

.repository-permissions__button--primary:hover:not(.v-btn--disabled),
.repository-permissions__button--primary:focus-visible:not(.v-btn--disabled),
.repository-permissions__button--danger:hover:not(.v-btn--disabled),
.repository-permissions__button--danger:focus-visible:not(.v-btn--disabled) {
  filter: brightness(0.93);
}

.repository-permissions__actions-heading {
  text-align: right;
}

.repository-permissions__footer {
  display: flex;
  gap: 0.75rem;
}

.repository-permissions__footer :deep(.submit-button) {
  min-width: 160px;
}

@media (max-width: 720px) {
  .repository-permissions__row {
    grid-template-columns: minmax(0, 1fr);
    align-items: flex-start;
    gap: 0.5rem;
    padding: 1rem;
  }

  .repository-permissions__toggle,
  .repository-permissions__toggle-heading,
  .repository-permissions__row-actions,
  .repository-permissions__actions-heading {
    justify-content: flex-start;
    text-align: left;
  }
}
</style>
