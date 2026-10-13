<script lang="ts">
  import { formatBytes, formatCount, formatDuration } from "$lib/format";
  import { app } from "$lib/state.svelte";

  let path = $state("");

  $effect(() => {
    const id = app.hovered ?? app.focus;
    void app.version;
    if (id === null) {
      path = "";
      return;
    }
    app.detail(id).then((d) => {
      if ((app.hovered ?? app.focus) === id) path = d.path;
    }).catch(() => {});
  });

  const s = $derived(app.summary);
  const selectedBytes = $derived(app.selectionInfo.reduce((sum, n) => sum + n.size, 0));
</script>

<footer class="status caption num">
  {#if s}
    <span>{formatCount(s.files)} files · {formatCount(s.dirs)} folders</span>
    <span>{formatBytes(s.physical)} on disk · {formatBytes(s.logical)} in files</span>
    {#if s.volumeTotal !== null && s.volumeFree !== null}
      <span>{formatBytes(s.volumeFree)} free of {formatBytes(s.volumeTotal)}</span>
    {/if}
    {#if !s.fromFile}<span>Scanned in {formatDuration(s.elapsedMs)}</span>{/if}
    {#if app.selection.length > 1}<span class="sel">{app.selection.length} selected · {formatBytes(selectedBytes)}</span>{/if}
    {#if app.info?.elevated}<span class="admin">Administrator</span>{/if}
  {/if}
  <span class="path">{path}</span>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 30px;
    padding: 0 16px;
    white-space: nowrap;
  }
  .sel,
  .admin {
    color: var(--accent-text);
  }
  .path {
    flex: 1;
    min-width: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
  }
</style>
