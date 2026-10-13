<script lang="ts">
  import { api, type FileEntry } from "$lib/api";
  import { app } from "$lib/state.svelte";
  import FileList from "./FileList.svelte";

  let files = $state<FileEntry[] | null>(null);
  let scopeName = $state("");

  $effect(() => {
    void [app.version, app.zoomRoot, app.settings?.largeFileCount];
    load();
  });

  async function load() {
    try {
      const [list, scope] = await Promise.all([api.largestFiles(app.zoomRoot, app.settings?.largeFileCount ?? 100), app.detail(app.zoomRoot)]);
      files = list;
      scopeName = scope.path;
    } catch {
      files = [];
    }
  }
</script>

<div class="largest">
  <div class="bar-top caption">
    The {app.settings?.largeFileCount ?? 100} largest files in <strong>{scopeName}</strong>
  </div>
  <div class="body">
    {#if files === null}
      <div class="ring"></div>
    {:else}
      <FileList {files} ranked />
    {/if}
  </div>
</div>

<style>
  .largest {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .bar-top {
    padding: 8px 12px;
    border-bottom: 1px solid var(--stroke-divider);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar-top strong {
    color: var(--text-1);
  }
  .body {
    flex: 1;
    min-height: 0;
  }
</style>
