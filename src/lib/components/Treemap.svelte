<script lang="ts">
  import { api, type NodeDetail, type Rect } from "$lib/api";
  import { formatBytes, formatCount, formatPercent } from "$lib/format";
  import { app } from "$lib/state.svelte";

  let canvas: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let busy = $state(false);
  let empty = $state(false);

  let scale = 1;
  let renderToken = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  let hoverRect: Rect | null = null;
  let selectRect: Rect | null = null;
  let tip = $state<{ x: number; y: number; detail: NodeDetail } | null>(null);
  let rootSize = $state(0);

  // Re-render when the area, the zoom or the data changes.
  $effect(() => {
    void [width, height, app.zoomRoot, app.version, app.highlightExt];
    clearTimeout(timer);
    timer = setTimeout(render, 40);
  });

  // Follow the selection from other views.
  $effect(() => {
    const id = app.focus;
    void app.version;
    updateSelection(id);
  });

  async function render() {
    scale = window.devicePixelRatio || 1;
    const w = Math.round(width * scale);
    const h = Math.round(height * scale);
    if (w < 2 || h < 2) return;
    const token = ++renderToken;
    busy = true;
    try {
      const [pixels, root] = await Promise.all([
        api.renderTreemap(app.zoomRoot, w, h, app.highlightExt),
        app.detail(app.zoomRoot),
      ]);
      if (token !== renderToken) return;
      rootSize = root.info.size;
      empty = root.info.size === 0;
      canvas.width = overlay.width = w;
      canvas.height = overlay.height = h;
      canvas.getContext("2d")!.putImageData(new ImageData(pixels as Uint8ClampedArray<ArrayBuffer>, w, h), 0, 0);
      hoverRect = null;
      await updateSelection(app.focus);
    } finally {
      if (token === renderToken) busy = false;
    }
  }

  async function updateSelection(id: number | null) {
    selectRect = id === null ? null : await api.treemapRect(id).catch(() => null);
    draw();
  }

  function draw() {
    const ctx = overlay?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, overlay.width, overlay.height);
    const outline = (r: Rect, color: string, lineWidth: number) => {
      const lw = lineWidth * scale;
      ctx.lineWidth = lw;
      ctx.strokeStyle = color;
      ctx.strokeRect(r.left + lw / 2, r.top + lw / 2, r.right - r.left - lw, r.bottom - r.top - lw);
    };
    if (hoverRect) {
      outline(hoverRect, "rgba(0,0,0,0.55)", 3);
      outline(hoverRect, "rgba(255,255,255,0.95)", 1.5);
    }
    if (selectRect) {
      const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent-base").trim();
      outline(selectRect, "rgba(0,0,0,0.6)", 4);
      outline(selectRect, accent || "#0078d4", 2.5);
    }
  }

  // Pointer hit testing: at most one request in flight, always for the
  // latest pointer position.
  let pending: { x: number; y: number; cx: number; cy: number } | null = null;
  let inflight = false;

  async function pump() {
    if (inflight || !pending) return;
    inflight = true;
    const p = pending;
    pending = null;
    try {
      const id = await api.treemapHit(Math.floor(p.x), Math.floor(p.y));
      if (id !== app.hovered) {
        app.hovered = id;
        hoverRect = id === null ? null : await api.treemapRect(id);
        draw();
      }
      if (id === null) {
        tip = null;
      } else {
        const detail = await app.detail(id);
        tip = { x: p.cx, y: p.cy, detail };
      }
    } catch {
      tip = null;
    } finally {
      inflight = false;
      if (pending) pump();
    }
  }

  function onmove(e: PointerEvent) {
    pending = { x: e.offsetX * scale, y: e.offsetY * scale, cx: e.offsetX, cy: e.offsetY };
    pump();
  }

  function onleave() {
    pending = null;
    app.hovered = null;
    hoverRect = null;
    tip = null;
    draw();
  }

  async function hitAt(e: MouseEvent) {
    return api.treemapHit(Math.floor(e.offsetX * scale), Math.floor(e.offsetY * scale));
  }

  async function onclick(e: MouseEvent) {
    const id = await hitAt(e);
    if (id === null) return;
    if (e.ctrlKey) app.toggleSelect(id);
    else app.select(id);
  }

  async function ondblclick(e: MouseEvent) {
    const id = await hitAt(e);
    if (id !== null) app.zoomTo(id);
  }

  async function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    const id = await hitAt(e);
    if (id === null) return;
    if (!app.isSelected(id)) app.select(id);
    app.menu = { x: e.clientX, y: e.clientY, kind: "treemap" };
  }

  const tipStyle = $derived.by(() => {
    if (!tip) return "";
    const flipX = tip.x > width - 300;
    const flipY = tip.y > height - 110;
    return `left:${flipX ? tip.x - 16 : tip.x + 16}px;top:${flipY ? tip.y - 12 : tip.y + 20}px;transform:translate(${flipX ? "-100%" : "0"},${flipY ? "-100%" : "0"})`;
  });
</script>

<div class="treemap" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
  <canvas
    bind:this={overlay}
    class="overlay"
    style:width="{width}px"
    style:height="{height}px"
    onpointermove={onmove}
    onpointerleave={onleave}
    {onclick}
    {ondblclick}
    {oncontextmenu}
  ></canvas>

  {#if empty}
    <div class="empty caption">Nothing to show: this folder is empty.</div>
  {/if}
  {#if busy}
    <div class="busy"><div class="ring"></div></div>
  {/if}
  {#if tip}
    <div class="tip flyout" style={tipStyle}>
      <div class="name">{tip.detail.info.name}</div>
      <div class="num">
        {formatBytes(tip.detail.info.size)}
        {#if rootSize > 0}<span class="caption">· {formatPercent(tip.detail.info.size / rootSize)} of view</span>{/if}
        {#if tip.detail.info.isDir}<span class="caption">· {formatCount(tip.detail.info.files)} files</span>{/if}
      </div>
      <div class="caption path">{tip.detail.path}</div>
    </div>
  {/if}
</div>

<style>
  .treemap {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border-radius: var(--radius-surface);
    background: #000;
  }
  canvas {
    position: absolute;
    inset: 0;
    display: block;
  }
  .overlay {
    cursor: crosshair;
  }
  .busy {
    position: absolute;
    right: 12px;
    top: 12px;
    pointer-events: none;
  }
  .busy .ring {
    width: 20px;
    height: 20px;
    border-width: 2px;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: rgba(255, 255, 255, 0.7);
  }
  .tip {
    position: absolute;
    pointer-events: none;
    padding: 8px 12px;
    max-width: 420px;
    z-index: 5;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .path {
    word-break: break-all;
  }
</style>
