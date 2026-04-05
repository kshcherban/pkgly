<template>
  <div class="go-project-helper">
    <div v-if="project" class="project-info">
      <h3>{{ project.name }}</h3>
      <div class="module-path">
        <strong>Module Path:</strong>
        <code>{{ getModulePath() }}</code>
      </div>

      <div v-if="version" class="version-info">
        <strong>Version:</strong>
        <span class="version-badge">{{ version.version }}</span>
        <span class="release-date">
          {{ version.release_type }}
        </span>
      </div>

      <div class="go-commands">
        <h4>Go Module Commands</h4>
        <div class="command-group">
          <div class="command-item">
            <label>Add to project:</label>
            <div class="command">
              <code>go get {{ getModulePath() }}@{{ version?.version || 'latest' }}</code>
              <button
                class="copy-button"
                @click="copyGoGetCommand"
                title="Copy to clipboard"
              >
                📋
              </button>
            </div>
          </div>

          <div class="command-item">
            <label>Import in code:</label>
            <div class="command">
              <code>import "{{ getImportPath() }}"</code>
              <button
                class="copy-button"
                @click="copyImportPath"
                title="Copy to clipboard"
              >
                📋
              </button>
            </div>
          </div>

          <div class="command-item">
            <label>Download module:</label>
            <div class="command">
              <code>go mod download {{ getModulePath() }}</code>
              <button
                class="copy-button"
                @click="copyModDownloadCommand"
                title="Copy to clipboard"
              >
                📋
              </button>
            </div>
          </div>
        </div>
      </div>

      <div class="module-info">
        <h4>Module Information</h4>
        <div class="info-grid">
          <div class="info-item">
            <span class="label">Repository Type:</span>
            <span class="value">{{ repository.repository_type }}</span>
          </div>
          <div class="info-item">
            <span class="label">Storage:</span>
            <span class="value">{{ repository.storage_name }}</span>
          </div>
          <div class="info-item">
            <span class="label">Project Key:</span>
            <span class="value">{{ project.project_key }}</span>
          </div>
          <div class="info-item">
            <span class="label">Created:</span>
            <span class="value">{{ formatDate(project.created_at.toISOString()) }}</span>
          </div>
          <div class="info-item">
            <span class="label">Updated:</span>
            <span class="value">{{ formatDate(project.updated_at.toISOString()) }}</span>
          </div>
        </div>
      </div>

      <div class="description">
        <h4>Project Details</h4>
        <p>Go module: {{ getModulePath() }}</p>
        <p>Version: {{ version?.version || 'Latest' }}</p>
      </div>
    </div>

    <div v-else class="no-project">
      <p>No Go module selected. Select a module to view usage instructions.</p>
    </div>

    <div class="go-links">
      <h4>Go Resources</h4>
      <div class="links-grid">
        <a href="https://golang.org/doc/modules/gomod-ref" target="_blank" class="go-link">
          📖 Go Modules Reference
        </a>
        <a href="https://golang.org/cmd/go/#hdr-Module_maintenance_and_working_with_modules" target="_blank" class="go-link">
          🛠️ Module Commands
        </a>
        <a href="https://golang.org/pkg/" target="_blank" class="go-link">
          📚 Package Documentation
        </a>
        <a href="https://proxy.golang.org/" target="_blank" class="go-link">
          🌐 Go Module Proxy
        </a>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { Project, ProjectVersion } from "@/types/project";
import type { RepositoryWithStorageName } from "@/types/repository";
import type { PropType } from "vue";

const props = defineProps({
  project: {
    type: Object as PropType<Project>,
    required: true,
  },
  version: {
    type: Object as PropType<ProjectVersion>,
    required: false,
  },
  repository: {
    type: Object as PropType<RepositoryWithStorageName>,
    required: true,
  },
});

const getModulePath = () => {
  if (props.project.name) {
    // Try to construct a reasonable module path from project name
    // In a real implementation, this would come from project metadata
    return `example.com/${props.project.name}`;
  }
  return "example.com/module";
};

const getImportPath = () => {
  const modulePath = getModulePath();
  // For most Go modules, the import path is the same as the module path
  // Some modules might have subpackages, but we'll keep it simple for now
  return modulePath;
};

const formatDate = (dateString: string) => {
  return new Date(dateString).toLocaleDateString();
};

const copyToClipboard = async (text: string) => {
  try {
    await navigator.clipboard.writeText(text);
    // You could add a toast notification here
  } catch (err) {
    console.error("Failed to copy text: ", err);
  }
};

const copyGoGetCommand = () => {
  copyToClipboard(`go get ${getModulePath()}@${props.version?.version || 'latest'}`);
};

const copyImportPath = () => {
  copyToClipboard(`import "${getImportPath()}"`);
};

const copyModDownloadCommand = () => {
  copyToClipboard(`go mod download ${getModulePath()}`);
};
</script>

<style scoped lang="scss">
.go-project-helper {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.project-info {
  h3 {
    margin: 0 0 1rem 0;
    color: var(--color-text-primary);
    font-size: 1.5rem;
  }
}

.module-path {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1rem;

  strong {
    color: var(--color-text-secondary);
  }

  code {
    background-color: var(--color-background-secondary);
    padding: 0.25rem 0.5rem;
    border-radius: var(--border-radius);
    font-family: var(--font-family-mono);
    font-size: 0.9rem;
    color: var(--color-text-primary);
  }
}

.version-info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 1rem;

  strong {
    color: var(--color-text-secondary);
  }

  .version-badge {
    background-color: var(--color-primary);
    color: white;
    padding: 0.25rem 0.5rem;
    border-radius: var(--border-radius);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .release-date {
    color: var(--color-text-secondary);
    font-size: 0.85rem;
  }
}

.go-commands {
  h4 {
    margin: 0 0 1rem 0;
    color: var(--color-text-primary);
    font-size: 1.1rem;
  }
}

.command-group {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.command-item {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;

  label {
    font-size: 0.85rem;
    color: var(--color-text-secondary);
    font-weight: 500;
  }

  .command {
    display: flex;
    align-items: center;
    gap: 0.5rem;

    code {
      flex: 1;
      background-color: var(--color-background-secondary);
      padding: 0.5rem;
      border-radius: var(--border-radius);
      font-family: var(--font-family-mono);
      font-size: 0.85rem;
      color: var(--color-text-primary);
      border: 1px solid var(--color-border);
    }

    .copy-button {
      background: none;
      border: 1px solid var(--color-border);
      border-radius: var(--border-radius);
      padding: 0.5rem;
      cursor: pointer;
      font-size: 0.85rem;
      transition: background-color 0.2s;

      &:hover {
        background-color: var(--color-background-secondary);
      }
    }
  }
}

.module-info {
  h4 {
    margin: 0 0 1rem 0;
    color: var(--color-text-primary);
    font-size: 1.1rem;
  }
}

.info-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 0.75rem;
}

.info-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem;
  background-color: var(--color-background-secondary);
  border-radius: var(--border-radius);

  .label {
    font-weight: 500;
    color: var(--color-text-secondary);
  }

  .value {
    color: var(--color-text-primary);

    &.link {
      color: var(--color-primary);
      text-decoration: none;

      &:hover {
        text-decoration: underline;
      }
    }
  }
}

.description {
  h4 {
    margin: 0 0 0.5rem 0;
    color: var(--color-text-primary);
    font-size: 1.1rem;
  }

  p {
    margin: 0;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
}

.no-project {
  padding: 2rem;
  text-align: center;
  background-color: var(--color-background-secondary);
  border-radius: var(--border-radius);
  border: 1px dashed var(--color-border);

  p {
    margin: 0;
    color: var(--color-text-secondary);
  }
}

.go-links {
  h4 {
    margin: 0 0 1rem 0;
    color: var(--color-text-primary);
    font-size: 1.1rem;
  }
}

.links-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 0.75rem;
}

.go-link {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem;
  background-color: var(--color-background-secondary);
  border-radius: var(--border-radius);
  text-decoration: none;
  color: var(--color-text-primary);
  border: 1px solid var(--color-border);
  transition: all 0.2s;

  &:hover {
    background-color: var(--color-primary);
    color: white;
    border-color: var(--color-primary);
  }
}

@media (max-width: 768px) {
  .info-grid {
    grid-template-columns: 1fr;
  }

  .links-grid {
    grid-template-columns: 1fr;
  }

  .command-item .command {
    flex-direction: column;
    align-items: stretch;

    .copy-button {
      align-self: flex-end;
    }
  }
}
</style>