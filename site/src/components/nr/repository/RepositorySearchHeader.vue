<!-- ABOUTME: Provides repository/package search and an accessible syntax-help dialog. -->
<!-- ABOUTME: Applies task-oriented search recipes to the shared search input. -->
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
        ref="helpButton"
        type="button"
        class="search-help-button"
        data-testid="search-help-button"
        @click="toggleSearchHelp"
        :aria-expanded="showSearchHelp"
        aria-controls="search-help-modal"
        title="Search syntax help">
        <v-icon aria-hidden="true">mdi-help-circle-outline</v-icon>
        <span class="sr-only">Search syntax help</span>
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
            ref="closeButton"
            type="button"
            class="modal-close"
            @click="closeSearchHelp"
            aria-label="Close search help">
            ×
          </button>
        </header>
        <div class="search-help-modal__content">
          <section aria-labelledby="searchRecipesTitle">
            <h4 id="searchRecipesTitle">Common searches</h4>
            <div class="recipes-grid">
              <button
                type="button"
                class="example-card"
                data-testid="search-recipe-package"
                @click="applyExample('package:express')">
                <span>Find a package</span>
                <code>package:express</code>
              </button>
              <button
                type="button"
                class="example-card"
                data-testid="search-recipe-repository"
                @click="applyExample('repo:npm-hosted')">
                <span>Filter by repository</span>
                <code>repo:npm-hosted</code>
              </button>
              <button
                type="button"
                class="example-card"
                data-testid="search-recipe-type"
                @click="applyExample('type:helm')">
                <span>Filter by repository type</span>
                <code>type:helm</code>
              </button>
              <button
                type="button"
                class="example-card"
                data-testid="search-recipe-version"
                @click="applyExample('version:>=1.0.0')">
                <span>Filter by version</span>
                <code>version:&gt;=1.0.0</code>
              </button>
              <button
                type="button"
                class="example-card"
                data-testid="search-recipe-combined"
                @click="applyExample('package:express version:>=4.0.0 type:npm')">
                <span>Combine filters</span>
                <code>package:express version:&gt;=4.0.0 type:npm</code>
              </button>
            </div>
          </section>
          <section>
            <h4>Quick text search</h4>
            <p>Match package, version, repository, or storage names.</p>
            <button
              type="button"
              class="example-chip"
              data-testid="search-example-basic"
              @click="applyExample('gin')">
              <code>gin</code>
              <span>Simple text search</span>
            </button>
          </section>
          <details class="search-reference">
            <summary>Syntax reference</summary>
            <section>
            <h4>Fields and aliases</h4>
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
            <h4>Tips</h4>
            <ul class="tips-list">
              <li>Combine multiple filters with spaces, e.g., <code>repo:npm-prod version:&gt;=2.0</code>.</li>
              <li>Use quotes for values containing spaces: <code>package:"@scope/pkg"</code>.</li>
              <li>Filters are case-insensitive.</li>
            </ul>
            </section>
          </details>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";

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
const helpButton = ref<HTMLButtonElement | null>(null);
const closeButton = ref<HTMLButtonElement | null>(null);
const internalValue = computed({
  get: () => props.modelValue,
  set: (value: string) => emit("update:modelValue", value),
});

function toggleSearchHelp() {
  if (showSearchHelp.value) {
    closeSearchHelp();
    return;
  }
  showSearchHelp.value = true;
  window.addEventListener("keydown", handleKeydown);
  void nextTick(() => closeButton.value?.focus());
}

function closeSearchHelp() {
  if (!showSearchHelp.value) {
    return;
  }
  showSearchHelp.value = false;
  window.removeEventListener("keydown", handleKeydown);
  void nextTick(() => helpButton.value?.focus());
}

function applyExample(query: string) {
  emit("update:modelValue", query);
  closeSearchHelp();
}

function clearSearch() {
  emit("update:modelValue", "");
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeSearchHelp();
  }
}

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleKeydown);
});
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
  border-radius: var(--nr-radius-round);
  border: 1px solid var(--nr-border-color, rgba(0, 0, 0, 0.12));
  background: var(--nr-background-primary, #fff);
  color: var(--nr-accent, #03a9f4);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color var(--nr-transition-fast), color var(--nr-transition-fast);
}

.search-help-button:hover,
.search-help-button:focus-visible {
  background: var(--nr-accent, #03a9f4);
  color: var(--nr-background-primary, #fff);
  box-shadow: var(--nr-focus-ring);
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
  border-radius: var(--nr-radius-lg);
  box-shadow: 0 20px 45px rgba(15, 23, 42, 0.25);
  width: min(48rem, 100%);
  max-height: 85vh;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--nr-spacing-lg);
  padding: var(--nr-spacing-lg);
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
  gap: var(--nr-spacing-lg);
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

.recipes-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--nr-spacing-sm);
  margin-top: var(--nr-spacing-sm);
}

.example-card,
.example-chip {
  border: 1px solid rgba(15, 23, 42, 0.12);
  background: var(--nr-background-secondary, #f8fafc);
  border-radius: var(--nr-radius-lg);
  padding: var(--nr-spacing-md);
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  text-align: left;
  cursor: pointer;
  transition: border-color var(--nr-transition-fast), box-shadow var(--nr-transition-fast);
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
  box-shadow: var(--nr-focus-ring);
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

.search-reference {
  border-top: 1px solid var(--nr-border-color);
  padding-top: var(--nr-spacing-md);
}

.search-reference summary {
  color: var(--nr-primary);
  cursor: pointer;
  font-weight: var(--nr-font-weight-medium);
}

.search-reference section {
  margin-top: var(--nr-spacing-md);
}

@media screen and (max-width: 900px) {
  .repository-search-header__controls {
    flex-direction: column;
    align-items: stretch;
  }

  .search-help-button {
    width: 100%;
    border-radius: var(--nr-radius-lg);
  }

  .recipes-grid {
    grid-template-columns: 1fr;
  }
}
</style>
