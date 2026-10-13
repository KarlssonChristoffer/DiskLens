<script lang="ts">
  import { formatCount } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import FileList from "./FileList.svelte";
  import Icon from "./Icon.svelte";

  // Results can contain items deleted since; drop them when data changes.
  let shown = $derived.by(() => {
    void app.version;
    return app.searchResults ?? [];
  });
</script>

<div class="search">
  <div class="bar-top">
    <span class="caption">{formatCount(shown.length)} matches for <strong>{app.searchQuery}</strong></span>
    <button class="btn subtle" onclick={() => (app.dialog = { kind: "search" })}><Icon name="search" size={16} /> New search</button>
  </div>
  <div class="body">
    {#if shown.length}
      <FileList files={shown} />
    {:else}
      <div class="empty caption">Nothing matched.</div>
    {/if}
  </div>
</div>

<style>
  .search {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .bar-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px 4px 12px;
    border-bottom: 1px solid var(--stroke-divider);
  }
  .bar-top strong {
    color: var(--text-1);
  }
  .bar-top .btn {
    min-height: 28px;
  }
  .body {
    flex: 1;
    min-height: 0;
  }
  .empty {
    padding: 24px;
    text-align: center;
  }
</style>
