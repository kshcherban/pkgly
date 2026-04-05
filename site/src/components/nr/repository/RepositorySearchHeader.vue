<template>
  <div class="repository-search-header">
    <div class="repository-search-header__controls">
      <div class="repository-search-header__input">
        <v-text-field
          v-model="internalValue"
          data-testid="repository-search-input"
          class="repository-search"
          :placeholder="placeholder"
          aria-label="Search repositories or packages"
          variant="outlined"
          density="comfortable"
          clearable
          hide-details
          :autofocus="autofocus"
          prepend-inner-icon="mdi-magnify"
          @click:clear="clearSearch" />
      </div>
      <button
        type="button"
        class="search-help-button"
        data-testid="search-help-button"
        @click="toggleSearchHelp"
        :aria-expanded="showSearchHelp"
        aria-controls="search-help-modal"
        title="Search syntax help">
        <span class="sr-only">Search syntax help</span>
        ?
      </button>
    </div>

    <div
      v-if="showSearchHelp"
      class="search-help-overlay"
      data-testid="search-help-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="searchHelpTitle"
      @click.self="closeSearchHelp">
      <div class="search-help-modal">
        <header class="search-help-modal__header">
          <h3 id="searchHelpTitle">Search Syntax Guide</h3>
          <button
            type="button"
            class="modal-close"
            @click="closeSearchHelp"
            aria-label="Close search help">
            ×
          </button>
        </header>
        <div class="search-help-modal__content">
          <section>
            <h4>Basic Search</h4>
            <p>Enter any text to match package names, versions, repository names, or storage names.</p>
            <button
              type="button"
              class="example-chip"
              data-testid="search-example-basic"
              @click="applyExample('gin')">
              <code>gin</code>
              <span>Simple text search</span>
            </button>
          </section>
          <section>
            <h4>Field Filters</h4>
            <table class="syntax-table">
              <thead>
                <tr>
                  <th>Field</th>
                  <th>Description</th>
                  <th>Example</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <td><code>package:</code>, <code>pkg:</code></td>
                  <td>Filter by package name</td>
                  <td><code>package:express</code></td>
                </tr>
                <tr>
                  <td><code>version:</code>, <code>v:</code></td>
                  <td>Filter by version</td>
                  <td><code>version:&gt;=1.0.0</code></td>
                </tr>
                <tr>
                  <td><code>repository:</code>, <code>repo:</code></td>
                  <td>Filter by repository name</td>
                  <td><code>repo:npm-hosted</code></td>
                </tr>
                <tr>
                  <td><code>type:</code></td>
                  <td>Filter by repository type</td>
                  <td><code>type:helm</code></td>
                </tr>
                <tr>
                  <td><code>storage:</code></td>
                  <td>Filter by storage name</td>
                  <td><code>storage:primary</code></td>
                </tr>
              </tbody>
            </table>
          </section>
          <section>
            <h4>Version Operators</h4>
            <ul class="operator-list">
              <li><code>&gt;</code>, <code>&gt;=</code>, <code>&lt;</code>, <code>&lt;=</code> — numeric or semantic comparisons</li>
              <li><code>=</code> — exact match (default)</li>
              <li><code>~</code> — contains search (e.g., <code>version:~beta</code>)</li>
              <li><code>^</code>, <code>~</code> prefix — semantic ranges (e.g., <code>version:^1.5</code>)</li>
            </ul>
          </section>
          <section>
            <h4>Examples</h4>
            <div class="examples-grid">
              <button
                type="button"
                class="example-card"
                data-testid="search-example-version"
                @click="applyExample('package:gin version:>1.0')">
                <code>package:gin version:&gt;1.0</code>
                <span>Gin packages newer than 1.0</span>
              </button>
              <button
                type="button"
                class="example-card"
                @click="applyExample('package:spring type:maven')">
                <code>package:spring type:maven</code>
                <span>Spring artifacts in Maven repositories</span>
              </button>
              <button
                type="button"
                class="example-card"
                @click="applyExample('nginx repo:docker-prod')">
                <code>nginx repo:docker-prod</code>
                <span>Nginx images in docker-prod</span>
              </button>
              <button
                type="button"
                class="example-card"
                @click="applyExample('package:~express version:>=4.0.0 type:npm')">
                <code>package:~express version:&gt;=4.0.0 type:npm</code>
                <span>Express packages ≥ 4.0.0 in npm repos</span>
              </button>
            </div>
          </section>
          <section>
            <h4>Tips</h4>
            <ul class="tips-list">
              <li>Combine multiple filters with spaces, e.g., <code>repo:npm-prod version:&gt;=2.0</code>.</li>
              <li>Use quotes for values containing spaces: <code>package:"@scope/pkg"</code>.</li>
              <li>Filters are case-insensitive.</li>
            </ul>
          </section>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    autofocus?: boolean;
    placeholder?: string;
  }>(),
  {
    modelValue: "",
    autofocus: false,
    placeholder: "Search packages or repositories",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

const showSearchHelp = ref(false);
const internalValue = computed({
  get: () => props.modelValue,
  set: (value: string) => emit("update:modelValue", value),
});

function toggleSearchHelp() {
  showSearchHelp.value = !showSearchHelp.value;
}

function closeSearchHelp() {
  showSearchHelp.value = false;
}

function applyExample(query: string) {
  emit("update:modelValue", query);
  closeSearchHelp();
}

function clearSearch() {
  emit("update:modelValue", "");
}
</script>

<style scoped lang="scss">
.repository-search-header {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 1rem 0;
}

.repository-search-header__controls {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.repository-search-header__input {
  flex: 1 1 auto;
}

.repository-search {
  width: 100%;
}

.search-help-button {
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 50%;
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  background: var(--nr-background-primary, #fff);
  color: var(--nr-accent, #03a9f4);
  font-weight: 600;
  font-size: 1.2rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color 0.2s ease, color 0.2s ease, transform 0.2s ease;
}

.search-help-button:hover,
.search-help-button:focus-visible {
  background: var(--nr-accent, #03a9f4);
  color: var(--nr-background-primary, #fff);
  transform: translateY(-1px);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

.search-help-overlay {
  position: fixed;
  inset: 0;
  background: rgba(17, 24, 39, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.25rem;
  z-index: 30;
}

.search-help-modal {
  background: var(--nr-background-primary, #fff);
  color: var(--nr-text-color, #1f2937);
  border-radius: 0.75rem;
  box-shadow: 0 20px 45px rgba(15, 23, 42, 0.25);
  width: min(48rem, 100%);
  max-height: 85vh;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
  padding: 1.5rem;
}

.search-help-modal__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
}

.search-help-modal__content {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.modal-close {
  border: none;
  background: transparent;
  font-size: 1.5rem;
  cursor: pointer;
  color: inherit;
  transition: transform 0.2s ease, color 0.2s ease;
}

.modal-close:hover,
.modal-close:focus-visible {
  transform: scale(1.1);
  color: var(--nr-accent, #03a9f4);
}

.syntax-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.95rem;
}

.syntax-table th,
.syntax-table td {
  padding: 0.5rem;
  border-bottom: 1px solid rgba(15, 23, 42, 0.12);
  text-align: left;
}

.syntax-table code {
  background: rgba(15, 23, 42, 0.08);
  padding: 0.2rem 0.35rem;
  border-radius: 0.25rem;
}

.examples-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 0.75rem;
}

.example-card,
.example-chip {
  border: 1px solid rgba(15, 23, 42, 0.12);
  background: var(--nr-background-secondary, #f8fafc);
  border-radius: 0.5rem;
  padding: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  text-align: left;
  cursor: pointer;
  transition: border-color 0.2s ease, box-shadow 0.2s ease, transform 0.2s ease;
  color: inherit;
}

.example-chip {
  display: inline-flex;
  flex-direction: row;
  align-items: center;
  gap: 0.5rem;
  width: fit-content;
}

.example-card:hover,
.example-card:focus-visible,
.example-chip:hover,
.example-chip:focus-visible {
  border-color: var(--nr-accent, #03a9f4);
  box-shadow: 0 0 0 3px rgba(100, 116, 139, 0.15);
  transform: translateY(-1px);
}

.example-card code,
.example-chip code {
  background: rgba(15, 23, 42, 0.08);
  padding: 0.25rem 0.4rem;
  border-radius: 0.25rem;
  font-size: 0.85rem;
}

.operator-list,
.tips-list {
  margin: 0;
  padding-left: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  font-size: 0.95rem;
}

@media screen and (max-width: 900px) {
  .repository-search-header__controls {
    flex-direction: column;
    align-items: stretch;
  }

  .search-help-button {
    width: 100%;
    border-radius: 0.75rem;
  }
}
</style>
