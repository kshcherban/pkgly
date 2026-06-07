<!-- ABOUTME: Manages persisted elevated roles and default repository permissions. -->
<!-- ABOUTME: Reuses shared permission controls and saves changes for an existing user. -->
<template>
  <section class="user-permissions">
    <UserPermissionFields v-model="userPermissions" />

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
import UserPermissionFields from "@/components/admin/user/UserPermissionFields.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import http from "@/http";
import {
  type InitialUserPermissions,
  type UserResponseType,
} from "@/types/base";
import { useAlertsStore } from "@/stores/alerts";
import { computed, ref, type PropType } from "vue";

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

  return !arraysEqual(
    userPermissions.value.default_repository_actions,
    props.user.default_repository_actions,
  );
});
const userPermissions = ref<InitialUserPermissions>({
  admin: props.user.admin,
  user_manager: props.user.user_manager,
  system_manager: props.user.system_manager,
  default_repository_actions: [...props.user.default_repository_actions],
});

function arraysEqual<T>(left: T[], right: T[]): boolean {
  return left.length === right.length && left.every((value) => right.includes(value));
}

async function save() {
  console.log("Saving User Permissions");
  const newPermissions = userPermissions.value;
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

.user-permissions__actions {
  display: flex;
  gap: 0.75rem;
}

.user-permissions__actions :deep(.submit-button) {
  min-width: 160px;
  align-self: flex-start;
}
</style>
