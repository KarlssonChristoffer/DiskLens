<script lang="ts">
  import { isEnabled, resolve, toolbar } from "$lib/commands";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let crumbs = $state<{ id: number; name: string }[]>([]);

  $effect(() => {
    void app.version;
    const root = app.zoomRoot;
    if (app.phase !== "results") return;
    app.detail(root).then(async (d) => {
      const names = await Promise.all(d.ancestry.map((id) => app.detail(id)));
      if (app.zoomRoot === root) crumbs = names.map((n) => ({ id: n.info.id, name: n.info.name }));
    }).catch(() => {});
  });

  function setMode(sizeMode: "physical" | "logical") {
    if (app.settings && app.settings.view.sizeMode !== sizeMode) app.updateSettings({ view: { ...app.settings.view, sizeMode } });
  }

  const items = $derived(
    toolbar.map((id) => {
      if (id === "-") return null;
      const c = resolve(id)!;
      return { id, c, enabled: isEnabled(c) };
    }),
  );
</script>

<div class="toolbar">
  <div class="buttons">
    {#each items as item}
      {#if item === null}
        <span class="sep"></span>
      {:else}
        <button
          class="btn subtle icon"
          class:danger={item.c.danger}
          title="{item.c.label}{item.c.shortcut ? ` (${item.c.shortcut})` : ''}"
          aria-label={item.c.label}
          disabled={!item.enabled}
          onclick={() => item.c.run()}
        >
          <Icon name={item.c.icon ?? "info"} />
        </button>
      {/if}
    {/each}
  </div>

  {#if app.phase === "results"}
    <nav class="crumbs" aria-label="Treemap location">
      {#each crumbs as c, i (c.id)}
        {#if i > 0}<span class="chev"><Icon name="chevronRight" size={12} /></span>{/if}
        <button class="crumb" class:current={i === crumbs.length - 1} onclick={() => (app.zoomRoot = c.id)}>{c.name}</button>
      {/each}
    </nav>
    <div class="segmented" role="radiogroup" aria-label="Size">
      <button class:on={app.settings?.view.sizeMode === "physical"} title="Space used on disk" onclick={() => setMode("physical")}>On disk</button>
      <button class:on={app.settings?.view.sizeMode === "logical"} title="File content size (Ctrl+L)" onclick={() => setMode("logical")}>File size</button>
    </div>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 12px;
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 1px;
    flex: none;
  }
  .btn.icon {
    width: 34px;
    min-height: 32px;
  }
  .btn.danger:not(:disabled) {
    color: var(--danger);
  }
  .btn:disabled {
    background: transparent;
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: var(--stroke-divider);
  }
  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    overflow: hidden;
    padding-left: 8px;
    border-left: 1px solid var(--stroke-divider);
  }
  .crumb {
    border: 0;
    background: transparent;
    padding: 4px 8px;
    border-radius: var(--radius-control);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 32px;
    font: 600 15px/22px var(--font-display);
  }
  .crumb:hover {
    background: var(--fill-subtle-hover);
    color: var(--text-1);
  }
  .crumb.current {
    color: var(--text-1);
    flex-shrink: 0;
    max-width: 50%;
  }
  .chev {
    display: inline-flex;
    color: var(--text-3);
    flex: none;
  }
</style>
