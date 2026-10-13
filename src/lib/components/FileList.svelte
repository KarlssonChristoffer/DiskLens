<script lang="ts">
  import type { FileEntry } from "$lib/api";
  import { formatBytes, formatDate, swatch } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let { files, ranked = false }: { files: FileEntry[]; ranked?: boolean } = $props();
  let anchor: number | null = null;

  function click(e: MouseEvent, f: FileEntry) {
    const id = f.info.id;
    if (e.ctrlKey) {
      app.toggleSelect(id);
      anchor = id;
    } else if (e.shiftKey && anchor !== null) {
      const a = files.findIndex((x) => x.info.id === anchor);
      const b = files.findIndex((x) => x.info.id === id);
      if (a >= 0 && b >= 0) {
        const range = files.slice(Math.min(a, b), Math.max(a, b) + 1);
        app.setSelection(range.map((x) => x.info.id), id, range.map((x) => x.info));
      }
    } else {
      app.select(id, f.info);
      anchor = id;
    }
  }

  function oncontext(e: MouseEvent, f: FileEntry) {
    e.preventDefault();
    if (!app.isSelected(f.info.id)) app.select(f.info.id, f.info);
    app.menu = { x: e.clientX, y: e.clientY, kind: "tree" };
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const i = files.findIndex((f) => f.info.id === app.focus);
    const next = files[Math.max(0, Math.min(files.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))];
    if (next) app.select(next.info.id, next.info);
    e.preventDefault();
    e.stopPropagation();
  }

  const folderOf = (path: string) => path.slice(0, path.lastIndexOf("\\") + 1);
</script>

<div class="list" class:stripes={app.settings?.listStripes} tabindex="0" role="listbox" aria-multiselectable="true" {onkeydown}>
  {#each files as f, i (f.info.id)}
    <div
      class="file"
      class:odd={i % 2 === 1}
      class:selected={app.isSelected(f.info.id)}
      class:focused={app.focus === f.info.id}
      role="option"
      aria-selected={app.isSelected(f.info.id)}
      tabindex="-1"
      onclick={(e) => click(e, f)}
      onkeydown={() => {}}
      oncontextmenu={(e) => oncontext(e, f)}
    >
      {#if ranked}<span class="rank caption num">{i + 1}</span>{/if}
      {#if f.info.isDir}
        <span class="folder"><Icon name="folder" size={16} /></span>
      {:else}
        <span class="dot" style:background={swatch(f.info.color)}></span>
      {/if}
      <div class="main">
        <div class="name">{f.info.name}</div>
        <div class="caption path">{folderOf(f.path)}</div>
      </div>
      <div class="right">
        <div class="num">{formatBytes(f.info.size)}</div>
        <div class="caption">{formatDate(f.info.modified)}</div>
      </div>
    </div>
  {/each}
</div>

<style>
  .list {
    height: 100%;
    overflow: auto;
    padding: 4px;
    outline: none;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-control);
    position: relative;
    content-visibility: auto;
    contain-intrinsic-size: auto 46px;
  }
  .stripes .file.odd {
    background: var(--fill-subtle-pressed);
  }
  .file:hover {
    background: var(--fill-subtle-hover);
  }
  .file.selected,
  .stripes .file.selected {
    background: var(--accent-subtle);
  }
  .file.focused::before {
    content: "";
    position: absolute;
    left: 0;
    top: 14px;
    bottom: 14px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .rank {
    width: 32px;
    text-align: right;
    flex: none;
  }
  .folder {
    color: #e8a73a;
    display: inline-flex;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 3px;
    flex: none;
  }
  .main {
    flex: 1;
    min-width: 0;
  }
  .name,
  .path {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .right {
    text-align: right;
    flex: none;
  }
</style>
