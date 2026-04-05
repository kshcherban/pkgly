<template>
  <div class="proxy-cache-notice" data-testid="proxy-cache-notice">
    <strong>Proxy caching is always enabled.</strong>
    Pkgly stores every downloaded artifact locally so that search, security policies, and quotas stay accurate.
    If caches fall behind, follow the reindex playbook to repopulate metadata.
  </div>
</template>

<style scoped lang="scss">
.proxy-cache-notice {
  border-left: 4px solid var(--v-theme-primary, #4c6ef5);
  padding: 0.75rem 1rem;
  border-radius: 4px;
  background: rgba(76, 110, 245, 0.06);
  color: var(--v-theme-on-surface, inherit);
  font-size: 0.9rem;
  line-height: 1.4;

  strong {
    display: block;
    margin-bottom: 0.25rem;
  }
}
</style>
