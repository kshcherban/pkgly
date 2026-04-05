<template>
  <tr class="browse__row browse__row--skeleton">
    <td class="browse__cell browse__cell--name">
      <div class="browse__cell-content">
        <span class="browse__skeleton-icon" />
        <span class="browse__skeleton-bar" />
      </div>
    </td>
    <td class="browse__cell browse__cell--meta">
      <span class="browse__skeleton-bar browse__skeleton-bar--meta" />
    </td>
  </tr>
</template>

<script setup lang="ts">
import "./browse.scss";
</script>
