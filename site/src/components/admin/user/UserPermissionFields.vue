<!-- ABOUTME: Renders reusable elevated-role and default repository permission controls. -->
<!-- ABOUTME: Emits API-shaped permission values for user creation and editing forms. -->
<template>
  <section class="user-permission-fields">
    <header>
      <h2 class="text-h5 mb-1">User Permission</h2>
      <p class="text-body-2 text-medium-emphasis">
        Configure the user’s elevated roles and default repository access.
      </p>
    </header>

    <div class="user-permission-fields__grid">
      <article class="user-permission-fields__card">
        <h3 class="text-subtitle-1 mb-1">Primary Permissions</h3>
        <p class="text-body-2 text-medium-emphasis">General permissions for Pkgly.</p>
        <div class="user-permission-fields__switches">
          <SwitchInput
            id="admin"
            :model-value="modelValue.admin"
            :disabled="disabled"
            @update:model-value="updateRole('admin', $event)">
            <template #comment>
              Admins have full control over the system.
              <br />
              <small>All other permissions are ignored.</small>
            </template>
            Admin
          </SwitchInput>
          <SwitchInput
            id="userManager"
            :model-value="modelValue.user_manager"
            :disabled="disabled"
            @update:model-value="updateRole('user_manager', $event)">
            <template #comment>
              Can create, edit, and remove users.
              <br />
              <small>Admins can only be edited by admins.</small>
            </template>
            User Manager
          </SwitchInput>
          <SwitchInput
            id="systemManager"
            :model-value="modelValue.system_manager"
            :disabled="disabled"
            @update:model-value="updateRole('system_manager', $event)">
            <template #comment>
              Can create, edit, and remove storages and repositories with full read/write access.
            </template>
            System Manager
          </SwitchInput>
        </div>
      </article>

      <article class="user-permission-fields__card">
        <h3 class="text-subtitle-1 mb-1">Default Repository Permissions</h3>
        <p class="text-body-2 text-medium-emphasis">
          Applied when the user does not have explicit repository permissions.
        </p>
        <div class="user-permission-fields__switches">
          <SwitchInput
            id="defaultRead"
            :model-value="hasAction(RepositoryActions.Read)"
            :disabled="disabled"
            @update:model-value="updateAction(RepositoryActions.Read, $event)">
            <template #comment>Can read artifacts on any repository.</template>
            Read
          </SwitchInput>
          <SwitchInput
            id="defaultWrite"
            :model-value="hasAction(RepositoryActions.Write)"
            :disabled="disabled"
            @update:model-value="updateAction(RepositoryActions.Write, $event)">
            <template #comment>Can write artifacts on any repository.</template>
            Write
          </SwitchInput>
          <SwitchInput
            id="defaultEdit"
            :model-value="hasAction(RepositoryActions.Edit)"
            :disabled="disabled"
            @update:model-value="updateAction(RepositoryActions.Edit, $event)">
            <template #comment>Can edit configuration on any repository.</template>
            Edit
          </SwitchInput>
        </div>
      </article>
    </div>
  </section>
</template>

<script lang="ts" setup>
import SwitchInput from "@/components/form/SwitchInput.vue";
import { RepositoryActions, type InitialUserPermissions } from "@/types/base";

const props = withDefaults(
  defineProps<{
    modelValue: InitialUserPermissions;
    disabled?: boolean;
  }>(),
  {
    disabled: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [permissions: InitialUserPermissions];
}>();

type Role = "admin" | "user_manager" | "system_manager";

function updateRole(role: Role, enabled: boolean) {
  emit("update:modelValue", {
    ...props.modelValue,
    [role]: enabled,
  });
}

function hasAction(action: RepositoryActions): boolean {
  return props.modelValue.default_repository_actions.includes(action);
}

function updateAction(action: RepositoryActions, enabled: boolean) {
  const actions = props.modelValue.default_repository_actions.filter(
    (existing) => existing !== action,
  );
  if (enabled) {
    actions.push(action);
  }
  emit("update:modelValue", {
    ...props.modelValue,
    default_repository_actions: actions,
  });
}
</script>

<style lang="scss" scoped>
.user-permission-fields {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.user-permission-fields__grid {
  display: grid;
  gap: 1.25rem;
}

@media (min-width: 960px) {
  .user-permission-fields__grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.user-permission-fields__card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 12px;
  background: var(--nr-surface);
  border: 1px solid var(--nr-border-muted);
}

.user-permission-fields__switches {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}
</style>
