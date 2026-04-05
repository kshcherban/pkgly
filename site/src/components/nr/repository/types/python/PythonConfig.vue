<template>
  <form class="python-config" @submit.prevent="save">
    <DropDown
      v-model="selectedType"
      :options="typeOptions"
      :disabled="!isCreate"
      class="full-width"
      required
    >Repository Type</DropDown>

    <div v-if="isProxy" class="proxy-routes">
      <div
        v-for="(route, index) in proxyRoutes"
        :key="index"
        class="route-row"
      >
        <TextInput v-model="route.url" required placeholder="https://pypi.org"
          >Upstream URL</TextInput
        >
        <TextInput v-model="route.name" placeholder="Optional label">Display Name</TextInput>
        <v-btn
          color="error"
          variant="flat"
          class="route-action text-none danger-hover"
          type="button"
          prepend-icon="mdi-delete"
          @click="removeRoute(index)"
        >Remove</v-btn>
      </div>
      <v-btn
        color="primary"
        variant="tonal"
        class="text-none align-self-start"
        type="button"
        prepend-icon="mdi-plus"
        @click="addRoute">
        Add Route
      </v-btn>
      <ProxyCacheNotice class="mt-2" />
    </div>

    <div v-if="isVirtual" class="virtual-config">
      <div class="virtual-settings">
        <DropDown
          id="virtual-resolution-order"
          v-model="virtualConfigSafe.resolution_order"
          :options="resolutionOrders"
        >Resolution Order</DropDown>
        <TextInput
          id="virtual-cache-ttl"
          v-model="cacheTtlString"
          type="text"
          inputmode="numeric"
          pattern="\\d*"
          placeholder="60"
        >Cache TTL (seconds)</TextInput>
        <DropDown
          id="virtual-publish-target"
          v-model="publishTarget"
          :options="publishTargetOptions"
        >Publish target (hosted)</DropDown>
      </div>

      <div class="virtual-members mt-3">
        <div class="text-subtitle-2 mb-1">Member repositories</div>
        <div class="text-body-2 text-medium-emphasis mb-2">
          Members are queried in ascending priority. Hosted repositories should generally have the
          lowest numbers.
        </div>

        <div
          v-for="(member, index) in virtualMembers"
          :key="`${member.repository_id}-${index}`"
          class="virtual-member-row"
          :data-testid="`virtual-member-${index}`">
          <DropDown
            :id="`virtual-member-${index}`"
            v-model="member.repository_id"
            :options="optionsForRow(member.repository_id)"
            @update:model-value="() => syncMemberNames()">
            Repository
          </DropDown>
          <TextInput
            :id="`virtual-priority-${index}`"
            :model-value="member.priority.toString()"
            type="number"
            min="0"
            placeholder="0"
            @update:model-value="(val?: string) => { member.priority = Math.max(0, Number(val ?? 0) || 0); }">
            Priority
          </TextInput>
          <SwitchInput
            :id="`virtual-enabled-${index}`"
            :aria-label="`Enable member ${member.repository_name || member.repository_id}`"
            hide-label
            v-model="member.enabled" />
          <v-btn
            color="error"
            variant="flat"
            class="text-none danger-hover"
            prepend-icon="mdi-delete"
            :aria-label="`Remove member ${member.repository_name || 'member'}`"
            @click="removeVirtualMember(index)">
            Delete
          </v-btn>
        </div>

        <v-btn
          color="primary"
          variant="tonal"
          class="text-none mt-2 align-self-start"
          type="button"
          prepend-icon="mdi-plus"
          data-testid="virtual-add-member"
          @click="addVirtualMember">
          Add Member
        </v-btn>
      </div>
    </div>

    <SubmitButton
      v-if="!isCreate"
      :block="false"
      prepend-icon="mdi-content-save">
      Save
    </SubmitButton>
  </form>
</template>
<script setup lang="ts">
import { computed, defineProps, onMounted, reactive, ref, watch } from "vue";
import DropDown from "@/components/form/dropdown/DropDown.vue";
import TextInput from "@/components/form/text/TextInput.vue";
import SubmitButton from "@/components/form/SubmitButton.vue";
import http from "@/http";
import ProxyCacheNotice from "@/components/nr/repository/ProxyCacheNotice.vue";
import SwitchInput from "@/components/form/SwitchInput.vue";
import { defaultProxy, defaultVirtual, type PythonConfigType } from "./python";
import { useRepositoryStore } from "@/stores/repositories";
import type { RepositoryWithStorageName } from "@/types/repository";

const typeOptions = [
  { value: "Hosted", label: "Hosted" },
  { value: "Proxy", label: "Proxy" },
  { value: "Virtual", label: "Virtual" },
];

const props = defineProps({
  settingName: String,
  repository: {
    type: String,
    required: false,
  },
});

const value = defineModel<PythonConfigType>({
  default: { type: "Hosted" },
});

const selectedType = ref<string>(value.value?.type ?? "Hosted");
const isCreate = computed(() => !props.repository);
const isProxy = computed(() => value.value?.type === "Proxy");
const isVirtual = computed(() => value.value?.type === "Virtual");
const proxyRoutes = computed(() => {
  if (value.value?.type !== "Proxy") {
    return [] as ReturnType<typeof defaultProxy>["routes"];
  }
  return value.value.config.routes;
});
const repositoryStore = useRepositoryStore();
const repositories = ref<RepositoryWithStorageName[]>([]);
const pythonRepositories = computed(() =>
  repositories.value.filter(
    (repo) => repo.repository_type.toLowerCase() === "python" && repo.id !== props.repository,
  ),
);
const resolutionOrders = [{ value: "Priority", label: "Priority (priority asc)" }];
const virtualConfig = computed(() => (value.value?.type === "Virtual" ? value.value.config : undefined));
const virtualConfigSafe = computed(() => {
  if (!isVirtual.value) {
    return defaultVirtual();
  }
  ensureVirtualConfig();
  return virtualConfig.value!;
});
const virtualMembers = computed(() => virtualConfigSafe.value.member_repositories);
const cacheTtlString = computed({
  get: () => virtualConfigSafe.value.cache_ttl_seconds?.toString() ?? "60",
  set: (val: string) => {
    const parsed = Number(val);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return;
    }
    virtualConfigSafe.value.cache_ttl_seconds = parsed;
  },
});
const publishTarget = computed({
  get: () => (isVirtual.value ? virtualConfigSafe.value.publish_to ?? "" : ""),
  set: (val: string) => {
    if (!isVirtual.value) {
      return;
    }
    virtualConfigSafe.value.publish_to = val || null;
  },
});
const memberOptions = computed(() =>
  pythonRepositories.value.map((repo) => ({
    value: repo.id,
    label: `${repo.name} (${repo.storage_name})`,
  })),
);
const publishTargetOptions = computed(() => [
  { value: "", label: "Auto-select hosted member" },
  ...virtualMembers.value.map((member) => ({
    value: member.repository_id,
    label: member.repository_name || member.repository_id,
  })),
]);

function optionsForRow(currentId?: string) {
  const selected = new Set(virtualMembers.value.map((member) => member.repository_id));
  return memberOptions.value.filter((option) => !selected.has(option.value) || option.value === currentId);
}

function ensureVirtualConfig() {
  if (value.value?.type !== "Virtual") {
    value.value = {
      type: "Virtual",
      config: makeVirtualConfig(),
    };
  }
  if (!virtualConfig.value?.member_repositories) {
    value.value = {
      type: "Virtual",
      config: makeVirtualConfig(),
    };
  }
}

function addVirtualMember() {
  ensureVirtualConfig();
  const available = optionsForRow();
  const chosen = available.length > 0 ? available[0] : undefined;
  const newMemberId = chosen?.value ?? "";
  virtualMembers.value.push({
    repository_id: newMemberId,
    repository_name: chosen?.label ?? "",
    priority: Math.max(0, virtualMembers.value.length * 10),
    enabled: true,
  });
  syncMemberNames();
}

function removeVirtualMember(index: number) {
  virtualConfigSafe.value.member_repositories.splice(index, 1);
  syncMemberNames();
  const publish = virtualConfigSafe.value.publish_to;
  if (publish && !virtualMembers.value.some((member) => member.repository_id === publish)) {
    publishTarget.value = "";
  }
}

function syncMemberNames() {
  const repoMap = new Map(pythonRepositories.value.map((repo) => [repo.id, repo]));
  virtualMembers.value.forEach((member) => {
    const repo = repoMap.get(member.repository_id);
    if (repo) {
      member.repository_name = repo.name;
    }
  });
  const publish = virtualConfigSafe.value.publish_to;
  if (publish && !virtualMembers.value.some((member) => member.repository_id === publish)) {
    publishTarget.value = "";
  }
}

function normalizeValue() {
  if (!value.value || typeof value.value !== "object") {
    value.value = { type: "Hosted" };
  }
  if (value.value?.type === "Proxy") {
    const routes = value.value.config?.routes ?? [];
    value.value = {
      type: "Proxy",
      config: {
        routes,
      },
    };
    selectedType.value = "Proxy";
    return;
  }
  if (value.value?.type === "Virtual") {
    value.value = {
      type: "Virtual",
      config: makeVirtualConfig(value.value.config),
    };
    selectedType.value = "Virtual";
    syncMemberNames();
    return;
  }
  value.value = { type: "Hosted" };
  selectedType.value = "Hosted";
}

normalizeValue();

watch(selectedType, (newType) => {
  if (newType === "Proxy") {
    if (value.value?.type !== "Proxy") {
      value.value = {
        type: "Proxy",
        config: defaultProxy(),
      };
    }
  } else if (newType === "Virtual") {
    if (value.value?.type !== "Virtual") {
      value.value = {
        type: "Virtual",
        config: makeVirtualConfig(),
      };
    }
  } else {
    value.value = { type: "Hosted" };
  }
});

watch(
  virtualMembers,
  () => {
    if (!isVirtual.value) {
      return;
    }
    syncMemberNames();
  },
  { deep: true },
);

function ensureProxyConfig() {
  if (value.value?.type !== "Proxy") {
    value.value = {
      type: "Proxy",
      config: defaultProxy(),
    };
  }
}

function addRoute() {
  ensureProxyConfig();
  if (value.value?.type === "Proxy") {
    value.value.config.routes.push({ url: "", name: "" });
  }
}

function removeRoute(index: number) {
  if (value.value?.type === "Proxy") {
    value.value.config.routes.splice(index, 1);
  }
}

async function load() {
  repositories.value = await repositoryStore.getRepositories(true);
  if (!props.repository) {
    return;
  }
  try {
    const membersResponse = await http.get(`/api/repository/${props.repository}/virtual/members`);
    const data = membersResponse.data;
    value.value = {
      type: "Virtual",
      config: makeVirtualConfig({
        member_repositories: data.members?.map((member: any) => ({
          repository_id: member.repository_id,
          repository_name: member.repository_name,
          priority: member.priority,
          enabled: member.enabled,
        })) ?? [],
        resolution_order: data.resolution_order,
        cache_ttl_seconds: data.cache_ttl_seconds,
        publish_to: data.publish_to ?? null,
      }),
    };
    selectedType.value = "Virtual";
    syncMemberNames();
    return;
  } catch (error) {
    // fallback to standard config endpoint
  }
  try {
    const response = await http.get(`/api/repository/${props.repository}/config/python`);
    value.value = response.data;
    normalizeValue();
  } catch (error) {
    console.error(error);
  }
}

async function save() {
  if (!props.repository) {
    return;
  }
  try {
    if (value.value?.type === "Virtual") {
      await http.post(`/api/repository/${props.repository}/virtual/members`, {
        members: virtualMembers.value,
        resolution_order: virtualConfigSafe.value.resolution_order,
        cache_ttl_seconds: virtualConfigSafe.value.cache_ttl_seconds,
        publish_to: virtualConfigSafe.value.publish_to,
      });
    } else {
      await http.put(`/api/repository/${props.repository}/config/python`, value.value);
    }
  } catch (error) {
    console.error(error);
  }
}

onMounted(() => {
  if (!value.value) {
    value.value = { type: "Hosted" };
  }
  load();
});

function makeVirtualConfig(config?: import("./python").PythonVirtualConfigType): import("./python").PythonVirtualConfigType {
  const base = config ?? defaultVirtual();
  return reactive({
    member_repositories: [...base.member_repositories],
    resolution_order: base.resolution_order ?? "Priority",
    cache_ttl_seconds: base.cache_ttl_seconds ?? 60,
    publish_to: base.publish_to ?? null,
  }) as import("./python").PythonVirtualConfigType;
}
</script>

<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.python-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.full-width {
  width: 100%;
}
.proxy-routes {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.route-row {
  display: grid;
  column-gap: 0.75rem;
  row-gap: 0.5rem;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  align-items: stretch;

  :deep(.route-action) {
    --v-btn-height: 48px;
    margin: 0;
    width: 100%;
    height: 48px;
    min-height: 48px;
    max-height: 48px;
    align-self: start;
    justify-self: stretch;
  }
}
.virtual-config {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}
.virtual-settings {
  display: grid;
  gap: 0.75rem;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
}
.virtual-member-row {
  display: grid;
  gap: 0.5rem;
  grid-template-columns: minmax(240px, 1fr) 120px 140px auto;
  align-items: center;
}
</style>
