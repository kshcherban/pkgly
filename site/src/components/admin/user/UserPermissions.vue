<template>
  <section class="user-permissions">
    <header class="user-permissions__header">
      <div>
        <h2 class="text-h5 mb-1">User Permission</h2>
        <p class="text-body-2 text-medium-emphasis">
          Configure the user’s elevated roles and default repository access.
        </p>
      </div>
    </header>

    <div class="user-permissions__grid">
      <article class="user-permissions__card">
        <h3 class="text-subtitle-1 mb-1">Primary Permissions</h3>
        <p class="text-body-2 text-medium-emphasis">
          General permissions for Pkgly.
        </p>
        <div class="user-permissions__switches">
          <SwitchInput
            id="admin"
            v-model="userPermissions.admin">
            <template #comment>
              Admins have full control over the system.
              <br />
              <small>All other permissions are ignored.</small>
            </template>
            Admin
          </SwitchInput>
          <SwitchInput
            id="userManager"
            v-model="userPermissions.user_manager">
            <template #comment>
              Can create, edit, and remove users.
              <br />
              <small>Admins can only be edited by admins.</small>
            </template>
            User Manager
          </SwitchInput>
          <SwitchInput
            id="systemManager"
            v-model="userPermissions.system_manager">
            <template #comment>
              Can create, edit, and remove storages and repositories with full read/write access.
            </template>
            System Manager
          </SwitchInput>
        </div>
      </article>

      <article class="user-permissions__card">
        <h3 class="text-subtitle-1 mb-1">Default Repository Permissions</h3>
        <p class="text-body-2 text-medium-emphasis">
          Applied when the user does not have explicit repository permissions.
        </p>
        <div class="user-permissions__switches">
          <SwitchInput
            id="defaultRead"
            v-model="userPermissions.default_repository_permissions.can_read">
            <template #comment>Can read artifacts on any repository.</template>
            Read
          </SwitchInput>
          <SwitchInput
            id="defaultWrite"
            v-model="userPermissions.default_repository_permissions.can_write">
            <template #comment>Can write artifacts on any repository.</template>
            Write
          </SwitchInput>
          <SwitchInput
            id="defaultEdit"
            v-model="userPermissions.default_repository_permissions.can_edit">
            <template #comment>Can edit configuration on any repository.</template>
            Edit
          </SwitchInput>
        </div>
      </article>
    </div>

    <footer class="user-permissions__actions">
      <SubmitButton
        :block="false"
        :disabled="!hasChanged"
        prepend-icon="mdi-content-save"
        @click="save">
        Save
      </SubmitButton>
    </footer>
  </section>
</template>
<script lang="ts" setup>
import SubmitButton from "@/components/form/SubmitButton.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import http from "@/http";
import { type UserResponseType } from "@/types/base";
import { RepositoryActionsType } from "@/types/user";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref, watch, type PropType } from "vue";

const props = defineProps({
  user: {
    type: Object as PropType<UserResponseType>,
    required: true,
  },
});
const alerts = useAlertsStore();
const hasChanged = computed(() => {
  if (userPermissions.value.admin !== props.user.admin) {
    return true;
  }
  if (userPermissions.value.user_manager !== props.user.user_manager) {
    return true;
  }
  if (userPermissions.value.system_manager !== props.user.system_manager) {
    return true;
  }

  return !userPermissions.value.default_repository_permissions.equalsArray(
    props.user.default_repository_actions,
  );
});
const userPermissions = ref({
  admin: props.user.admin,
  user_manager: props.user.user_manager,
  system_manager: props.user.system_manager,
  default_repository_permissions: new RepositoryActionsType(props.user.default_repository_actions),
});
async function save() {
  console.log("Saving User Permissions");
  const newPermissions = {
    admin: userPermissions.value.admin,
    user_manager: userPermissions.value.user_manager,
    system_manager: userPermissions.value.system_manager,
    default_repository_actions: userPermissions.value.default_repository_permissions.asArray(),
  };
  console.log(`Saving: ${JSON.stringify(newPermissions)}`);
  try {
    await http.put(`/api/user-management/update/${props.user.id}/permissions`, newPermissions);
    alerts.success("Permissions saved", "Permissions have been saved.");
  } catch (error: any) {
    let text = "An error occurred while saving permissions.";
    if (error?.response?.data) {
      text = error.response.data;
    }
    alerts.error("Error saving permissions", text);
  }
}
</script>
<style lang="scss" scoped>
.user-permissions {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.user-permissions__grid {
  display: grid;
  gap: 1.25rem;
}

@media (min-width: 960px) {
  .user-permissions__grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.user-permissions__card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 12px;
  background: var(--nr-surface);
  border: 1px solid var(--nr-border-muted);
}

.user-permissions__switches {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.user-permissions__actions {
  display: flex;
  gap: 0.75rem;
}

.user-permissions__actions :deep(.submit-button) {
  min-width: 160px;
  align-self: flex-start;
}
</style>
