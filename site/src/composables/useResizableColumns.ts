import { onMounted, onBeforeUnmount, ref } from 'vue';

export interface ResizableColumnState {
  isResizing: boolean;
  startX: number;
  startWidth: number;
  columnIndex: number;
}

export function useResizableColumns(tableSelector: string, minColumnWidth = 80) {
  const state = ref<ResizableColumnState | null>(null);

  const cleanupResizers = () => {
    const table = document.querySelector(tableSelector);
    if (!table) return;
    table.querySelectorAll('.column-resizer').forEach((handle) => handle.remove());
  };

  const initResizable = () => {
    const table = document.querySelector(tableSelector);
    if (!table) return;

    cleanupResizers();

    const headers = table.querySelectorAll('th');
    headers.forEach((th, index) => {
      const headerEl = th as HTMLElement;
      // Create resize handle
      const resizer = document.createElement('div');
      resizer.className = 'column-resizer';
      resizer.style.cssText = `
        position: absolute;
        top: 0;
        right: 0;
        width: 8px;
        height: 100%;
        cursor: col-resize;
        user-select: none;
        z-index: 2;
      `;

      if (getComputedStyle(headerEl).position === 'static') {
        headerEl.style.position = 'relative';
      }

      resizer.addEventListener('mousedown', (e) => onMouseDown(e, headerEl, index));
      headerEl.appendChild(resizer);
    });
  };

  const onMouseDown = (e: MouseEvent, th: HTMLElement, index: number) => {
    e.preventDefault();
    state.value = {
      isResizing: true,
      startX: e.pageX,
      startWidth: th.offsetWidth,
      columnIndex: index,
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  };

  const onMouseMove = (e: MouseEvent) => {
    if (!state.value?.isResizing) return;

    const table = document.querySelector(tableSelector);
    if (!table) return;

    const th = table.querySelectorAll('th')[state.value.columnIndex];
    if (!th) return;

    const diff = e.pageX - state.value.startX;
    const newWidth = Math.max(minColumnWidth, state.value.startWidth + diff);
    (th as HTMLElement).style.width = `${newWidth}px`;
    (th as HTMLElement).style.minWidth = `${newWidth}px`;
    (th as HTMLElement).style.maxWidth = `${newWidth}px`;
  };

  const onMouseUp = () => {
    state.value = null;
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };

  onMounted(() => {
    // Delay to ensure DOM is ready
    setTimeout(initResizable, 100);
  });

  onBeforeUnmount(() => {
    cleanupResizers();
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
  });

  return {
    initResizable,
    cleanupResizers,
  };
}
