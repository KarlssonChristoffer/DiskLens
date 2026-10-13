<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { api, type Volume } from "$lib/api";
  import { runCommand } from "$lib/commands";
  import { formatBytes } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let volumes = $state<Volume[] | null>(null);
  const picked = new SvelteSet<string>();

  onMount(async () => {
    volumes = await api.listVolumes();
  });

  const name = (v: Volume) => {
    const letter = v.root.replace(/\\$/, "");
    const label = v.label || (v.kind === "network" ? "Network drive" : v.kind === "removable" ? "USB drive" : "Local Disk");
    return `${label} (${letter})`;
  };

  function toggle(root: string) {
    if (picked.has(root)) picked.delete(root);
    else picked.add(root);
  }

  const localDrives = $derived(volumes?.filter((v) => v.kind === "fixed").map((v) => v.root) ?? []);
</script>

<div class="start">
  <header>
    <div class="logo"><Icon name="dataTreemap" size={28} /></div>
    <div>
      <h1>DiskLens</h1>
      <p class="caption">See what's taking up space, down to the last file.</p>
    </div>
    {#if !app.info?.elevated}
      <button class="btn elevate" title="Scan everything, including protected system folders" onclick={() => runCommand("file.elevate")}>
        <Icon name="shieldKeyhole" /> Run as administrator
      </button>
    {/if}
  </header>

  <section>
    <div class="section-head">
      <h2>Drives</h2>
      <span class="caption">Click a drive to scan it, or tick several and scan them together.</span>
    </div>
    <div class="drives">
      {#if volumes === null}
        <div class="ring"></div>
      {:else}
        {#each volumes as v (v.root)}
          {@const used = v.totalBytes - v.freeBytes}
          <div class="drive card" class:picked={picked.has(v.root)}>
            <button class="main" onclick={() => app.scan([v.root])}>
              <span class="glyph"><Icon name="hardDriveLarge" size={40} /></span>
              <span class="meta">
                <span class="title">{name(v)}</span>
                <span class="bar"><span style:width="{(used / Math.max(v.totalBytes, 1)) * 100}%"></span></span>
                <span class="caption">{formatBytes(v.freeBytes)} free of {formatBytes(v.totalBytes)} · {v.fileSystem}</span>
              </span>
            </button>
            <label class="pick" title="Include in a multi-drive scan">
              <input type="checkbox" checked={picked.has(v.root)} onchange={() => toggle(v.root)} />
            </label>
          </div>
        {/each}
      {/if}
    </div>
  </section>

  <section class="actions">
    <button class="btn accent" disabled={picked.size === 0} onclick={() => app.scan([...picked])}>
      <Icon name="hardDrive" /> Scan {picked.size || ""} selected
    </button>
    <button class="btn" disabled={localDrives.length === 0} onclick={() => app.scan(localDrives)}>
      <Icon name="storage" /> All local drives
    </button>
    <button class="btn" onclick={() => runCommand("file.scanFolder")}>
      <Icon name="folderOpen" /> Scan a folder…
    </button>
    <button class="btn" onclick={() => app.loadResults()}>
      <Icon name="documentArrowUp" /> Load results…
    </button>
    {#if app.summary}
      <button class="btn subtle" onclick={() => (app.phase = "results")}>
        <Icon name="arrowLeft" /> Back to {app.summary.rootPath}
      </button>
    {/if}
  </section>

  {#if app.settings?.folderHistory.length}
    <section>
      <h2>Recent folders</h2>
      <div class="history">
        {#each app.settings.folderHistory as path}
          <button class="recent" onclick={() => app.scan([path])}>
            <Icon name="folder" size={16} />
            <span>{path}</span>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>

<style>
  .start {
    height: 100%;
    overflow: auto;
    padding: 32px max(32px, calc(50% - 460px));
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 32px;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 12px;
    background: var(--accent);
    color: var(--on-accent);
  }
  .elevate {
    margin-left: auto;
  }
  h1 {
    margin: 0;
    font: 600 28px/36px var(--font-display);
  }
  h2 {
    margin: 0;
    font: 600 14px/20px var(--font);
  }
  p {
    margin: 0;
  }
  section {
    margin-bottom: 24px;
  }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 12px;
  }
  section > h2 {
    margin-bottom: 8px;
  }
  .drives {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
  }
  .drive {
    position: relative;
    display: flex;
    transition: background 0.1s;
  }
  .drive.picked {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .drive:hover {
    background: var(--fill-control-hover);
  }
  .main {
    display: flex;
    align-items: center;
    gap: 16px;
    flex: 1;
    min-width: 0;
    padding: 16px 44px 16px 16px;
    border: 0;
    background: transparent;
    text-align: left;
  }
  .pick {
    position: absolute;
    top: 12px;
    right: 12px;
    padding: 4px;
  }
  .glyph {
    color: var(--text-2);
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .history {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .recent {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 0;
    background: transparent;
    padding: 6px 10px;
    border-radius: var(--radius-control);
    text-align: left;
    color: var(--text-1);
  }
  .recent :global(.icon) {
    color: #e8a73a;
  }
  .recent:hover {
    background: var(--fill-subtle-hover);
  }
  .recent span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
