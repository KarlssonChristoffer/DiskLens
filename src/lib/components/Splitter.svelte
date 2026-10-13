<script lang="ts">
  // A drag handle that adjusts a pixel size. `invert` makes dragging toward
  // the start grow the size (for panes anchored at the end).
  let {
    value = $bindable(),
    min,
    max,
    vertical = false,
    invert = false,
  }: { value: number; min: number; max: () => number; vertical?: boolean; invert?: boolean } = $props();

  let start = 0;
  let startValue = 0;
  let dragging = $state(false);

  function down(e: PointerEvent) {
    dragging = true;
    start = vertical ? e.clientX : e.clientY;
    startValue = value;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    const delta = (vertical ? e.clientX : e.clientY) - start;
    value = Math.round(Math.max(min, Math.min(max(), startValue + (invert ? -delta : delta))));
  }
</script>

<div
  class="splitter"
  class:vertical
  class:dragging
  role="separator"
  aria-orientation={vertical ? "vertical" : "horizontal"}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={() => (dragging = false)}
></div>

<style>
  .splitter {
    position: relative;
    cursor: row-resize;
  }
  .splitter.vertical {
    cursor: col-resize;
  }
  .splitter::after {
    content: "";
    position: absolute;
    left: 50%;
    top: 50%;
    width: 32px;
    height: 3px;
    transform: translate(-50%, -50%);
    border-radius: 2px;
    background: var(--text-3);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .splitter.vertical::after {
    width: 3px;
    height: 32px;
  }
  .splitter:hover::after,
  .splitter.dragging::after {
    opacity: 1;
  }
</style>
