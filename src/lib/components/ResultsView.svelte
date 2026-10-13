<script lang="ts">
  import { app, type Tab } from "$lib/state.svelte";
  import Duplicates from "./Duplicates.svelte";
  import ExtList from "./ExtList.svelte";
  import FolderTree from "./FolderTree.svelte";
  import Icon from "./Icon.svelte";
  import LargestFiles from "./LargestFiles.svelte";
  import SearchResults from "./SearchResults.svelte";
  import Splitter from "./Splitter.svelte";
  import Treemap from "./Treemap.svelte";
  import type { IconName } from "$lib/icons";

  const stored = (key: string, fallback: number) => {
    try {
      const v = Number(localStorage.getItem(key));
      return v > 0 ? v : fallback;
    } catch {
      return fallback;
    }
  };

  let mainHeight = $state(0);
  let topWidth = $state(0);
  let topHeight = $state(stored("disklens.topHeight", 360));
  let extWidth = $state(stored("disklens.extWidth", 320));

  $effect(() => {
    try {
      localStorage.setItem("disklens.topHeight", String(topHeight));
      localStorage.setItem("disklens.extWidth", String(extWidth));
    } catch {}
  });

  const showTypes = $derived(app.settings?.showFileTypes ?? true);
  const showMap = $derived(app.settings?.showTreemap ?? true);

  const tabs = $derived<{ id: Tab; label: string; icon: IconName }[]>([
    { id: "folders", label: "All files", icon: "folder" },
    { id: "largest", label: "Largest files", icon: "appsList" },
    { id: "duplicates", label: "Duplicates", icon: "documentCopy" },
    ...(app.searchResults ? [{ id: "search" as Tab, label: "Search results", icon: "search" as IconName }] : []),
  ]);
</script>

<div class="results">
  <div
    class="main"
    bind:clientHeight={mainHeight}
    style:grid-template-rows={showMap ? `${Math.min(topHeight, Math.max(mainHeight - 160, 120))}px 8px 1fr` : "1fr"}
  >
    <div
      class="top"
      bind:clientWidth={topWidth}
      style:grid-template-columns={showTypes ? `1fr 8px ${Math.min(extWidth, Math.max(topWidth - 360, 200))}px` : "1fr"}
    >
      <section class="card pane">
        <div class="tabs" role="tablist">
          {#each tabs as t (t.id)}
            <button class="tab" class:on={app.tab === t.id} role="tab" aria-selected={app.tab === t.id} onclick={() => (app.tab = t.id)}>
              <Icon name={t.icon} size={16} />
              {t.label}
            </button>
          {/each}
          {#if app.summary?.fromFile}<span class="badge caption">Loaded from file</span>{/if}
          {#if app.summary?.incomplete}<span class="badge caption">Incomplete scan</span>{/if}
        </div>
        <div class="panel" hidden={app.tab !== "folders"}><FolderTree /></div>
        <div class="panel" hidden={app.tab !== "largest"}><LargestFiles /></div>
        <div class="panel" hidden={app.tab !== "duplicates"}><Duplicates /></div>
        {#if app.searchResults}
          <div class="panel" hidden={app.tab !== "search"}><SearchResults /></div>
        {/if}
      </section>
      {#if showTypes}
        <Splitter vertical invert bind:value={extWidth} min={200} max={() => topWidth - 360} />
        <section class="card pane"><ExtList /></section>
      {/if}
    </div>
    {#if showMap}
      <Splitter bind:value={topHeight} min={120} max={() => mainHeight - 160} />
      <section class="card map"><Treemap /></section>
    {/if}
  </div>
</div>

<style>
  .results {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .main {
    flex: 1;
    min-height: 0;
    display: grid;
    padding: 0 12px 4px;
  }
  .top {
    display: grid;
    min-height: 0;
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px 0;
    border-bottom: 1px solid var(--stroke-divider);
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: transparent;
    padding: 6px 12px 10px;
    border-radius: var(--radius-control) var(--radius-control) 0 0;
    color: var(--text-2);
  }
  .tab:hover {
    color: var(--text-1);
    background: var(--fill-subtle-hover);
  }
  .tab.on {
    color: var(--text-1);
    font-weight: 600;
  }
  .tab.on::after {
    content: "";
    position: absolute;
    left: 50%;
    bottom: 0;
    width: 16px;
    height: 3px;
    transform: translateX(-50%);
    border-radius: 2px;
    background: var(--accent);
  }
  .badge {
    margin-left: auto;
    margin-bottom: 4px;
    padding: 1px 8px;
    border-radius: 10px;
    background: var(--accent-subtle);
    color: var(--text-1);
  }
  .badge + .badge {
    margin-left: 0;
  }
  .panel {
    flex: 1;
    min-height: 0;
  }
  .panel[hidden] {
    display: none;
  }
  .map {
    min-height: 0;
    padding: 0;
    overflow: hidden;
  }
</style>
