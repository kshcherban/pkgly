<template>
  <v-card
    v-if="form && model"
    class="editor-box"
    variant="outlined">
    <v-card-title class="editor-box__header">
      Generic Config Editor: {{ settingName }}
    </v-card-title>
    <v-card-text class="editor-box__content">
      <JsonSchemaForm
        :form="form"
        v-model="model" />
    </v-card-text>
    <v-divider />
    <v-card-actions class="justify-end">
      <SubmitButton
        :block="false"
        data-testid="generic-config-save"
        prepend-icon="mdi-content-save"
        @click="save">
        Save Configuration
      </SubmitButton>
    </v-card-actions>
  </v-card>
</template>
<script setup lang="ts">
import http from "@/http";
import { computed, ref, type PropType } from "vue";
import { useRepositoryStore } from "@/stores/repositories";
import JsonSchemaForm from "@/components/form/JsonSchemaForm.vue";
import { createForm, type RootSchema } from "@/lib/pkgly-jsf";
import SubmitButton from "@/components/form/SubmitButton.vue";

const schema = ref<RootSchema | undefined>(undefined);
const form = computed(() => {
  if (schema.value) {
    return createForm(schema.value);
  }
  return undefined;
});
const props = defineProps({
  settingName: String,
  repository: {
    type: Object as PropType<string>,
    required: false,
  },
});
const model = defineModel<any>();
const repositoryTypeStore = useRepositoryStore();
async function load() {
  if (!props.settingName) {
    throw new Error("settingName is required");
  }
  await repositoryTypeStore.getConfigSchema(props.settingName).then((response) => {
    schema.value = response as RootSchema;
    console.log(schema.value);
  });
  if (props.repository) {
    await http
      .get(`/api/repository/${props.repository}/config/${props.settingName}?default=true`)
      .then((response) => {
        model.value = response.data;
      })
      .catch((error) => {
        console.error(error);
      });
    if (!model.value) {
      loadDefault();
    }
  } else {
    loadDefault();
  }
}
async function loadDefault() {
  await http
    .get(`/api/repository/repository/${props.repository}/config/${props.settingName}`)
    .then((response) => {
      model.value = response.data;
    })
    .catch((error) => {
      console.error(error);
    });
}

async function save() {
  if (!props.repository || !props.settingName || !model.value) {
    console.error("Missing required properties for save");
    return;
  }
  try {
    await http.put(
      `/api/repository/${props.repository}/config/${props.settingName}`,
      model.value
    );
    await load();
  } catch (error) {
    console.error("Failed to save configuration", error);
  }
}

load();
</script>
<style scoped lang="scss">
@use "@/assets/styles/theme.scss" as *;

.editor-box {
  display: flex;
  flex-direction: column;
  gap: 0;
  overflow: hidden;
}

.editor-box__header {
  font-size: 1.125rem;
  font-weight: 600;
}

.editor-box__content {
  padding-top: 0;
}
</style>
