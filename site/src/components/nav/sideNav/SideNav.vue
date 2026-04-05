<template>
  <nav>
    <slot />
  </nav>
</template>
<style scoped lang="scss">
nav {
  margin: 0.5rem;
  margin-right: 0.5rem;
  border-top-right-radius: 8px;
  border-bottom-right-radius: 8px;
  width: 200px;
  background-color: var(--nr-surface);
  border: 1px solid var(--nr-border-color);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

#logoAndHome {
  img {
    width: 2rem;
    height: 2rem;
  }
  span {
    color: var(--nr-text-primary);
  }
}
.bottom {
  margin-top: auto;
}
</style>
