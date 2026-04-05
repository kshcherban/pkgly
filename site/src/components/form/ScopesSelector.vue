<template>
  <div>
    <div
      v-for="[parent, scopes] in scopeDescriptionsOrganized"
      :key="parent">
      <h2 class="groupName">{{ parent }}</h2>
      <div
        class="scopeGroup"
        :data-is-expanded="isExpanded(parent)">
        <div
          v-for="scope in scopes"
          :key="scope.key"
          class="scopeEntry">
          <span>{{ scope.name }}</span>
          <small>{{ scope.description }}</small>
          <SwitchInput
            :id="`scope-${scope.key}`"
            :model-value="isScopeSelected(scope)"
            class="scope-switch"
            @update:model-value="(value) => toggleScope(scope, value)">
            {{ isScopeSelected(scope) ? "Enabled" : "Enable" }}
          </SwitchInput>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { siteStore } from "@/stores/site";
import { type ScopeDescription } from "@/types/base";
import SwitchInput from "./SwitchInput.vue";
import { ref } from "vue";

const site = siteStore();
const expandedGroups = ref<Array<string>>([]);

function isExpanded(group: string) {
  return expandedGroups.value.includes(group);
}

const scopeDescriptions = ref<Array<ScopeDescription>>([]);
const scopeDescriptionsOrganized = ref<Map<string, Array<ScopeDescription>>>(new Map());
function organizeScopes() {
  const organized = new Map<string, Array<ScopeDescription>>();
  for (const scope of scopeDescriptions.value) {
    const parent = scope.parent || "Other";
    if (organized.has(parent)) {
      organized.get(parent)?.push(scope);
    } else {
      organized.set(parent, [scope]);
    }
  }
  scopeDescriptionsOrganized.value = organized;
}
async function getScopeDescriptions() {
  await site.getScopes().then((response) => {
    scopeDescriptions.value = response;
    organizeScopes();
  });
}
const model = defineModel<Array<ScopeDescription>>({
  required: true,
});
function removeDescription(scope: ScopeDescription) {
  model.value = model.value.filter((s) => s.key !== scope.key);
}
function addDescription(scope: ScopeDescription) {
  if (!isScopeSelected(scope)) {
    model.value = [...model.value, scope];
  }
}

function isScopeSelected(scope: ScopeDescription): boolean {
  return model.value.some((item) => item.key === scope.key);
}

function toggleScope(scope: ScopeDescription, enabled: boolean) {
  if (enabled) {
    addDescription(scope);
  } else {
    removeDescription(scope);
  }
}

getScopeDescriptions();
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme" as *;
.groupName {
  font-size: 1.5rem;
  margin: 1rem;
}
.scopeGroup {
  display: flex;
  flex-direction: row;
  flex-wrap: wrap;
  padding-left: 2rem;
}
.scopeEntry {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  margin: 0.5rem;

  :deep(.switch-wrapper) {
    margin: 0.5rem 0 0;
  }
}
</style>
