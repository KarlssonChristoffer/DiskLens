<script lang="ts">
  import type { Column, Settings, TreemapOptions, UserCleanup } from "$lib/api";
  import { app } from "$lib/state.svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";

  let { page = "general" }: { page?: string } = $props();

  // Edit a copy; nothing applies until Save.
  let s = $state<Settings>(structuredClone($state.snapshot(app.settings!)) as Settings);
  // svelte-ignore state_referenced_locally
  let tab = $state(page); // the page to open on; switching tabs is local
  let cleanupIndex = $state(0);
  let excludeDirs = $state(s.scan.excludeDirs.join("\n"));
  let excludeFiles = $state(s.scan.excludeFiles.join("\n"));
  let minSizeValue = $state(s.scan.minFileSize >= 1048576 ? s.scan.minFileSize / 1048576 : s.scan.minFileSize / 1024);
  let minSizeUnit = $state(s.scan.minFileSize >= 1048576 ? 1048576 : 1024);

  const tabs = [
    { id: "general", label: "General" },
    { id: "columns", label: "Folder list" },
    { id: "treemap", label: "Treemap" },
    { id: "scanning", label: "Scanning" },
    { id: "cleanups", label: "Cleanups" },
  ];

  const allColumns: { id: Column; label: string }[] = [
    { id: "physical", label: "Size on disk" },
    { id: "logical", label: "File size" },
    { id: "items", label: "Items" },
    { id: "files", label: "Files" },
    { id: "folders", label: "Folders" },
    { id: "modified", label: "Last change" },
    { id: "attributes", label: "Attributes" },
    { id: "owner", label: "Owner" },
  ];

  const DEFAULT_TREEMAP: TreemapOptions = {
    style: "KDirStat",
    grid: false,
    gridColor: [0, 0, 0],
    brightness: 0.88,
    height: 0.38,
    scaleFactor: 0.91,
    ambientLight: 0.13,
    lightX: -1,
    lightY: -1,
  };

  const sliders: { key: keyof TreemapOptions; label: string; min: number; max: number }[] = [
    { key: "brightness", label: "Brightness", min: 0.1, max: 1 },
    { key: "height", label: "Cushion height", min: 0, max: 1 },
    { key: "scaleFactor", label: "Nesting depth", min: 0.1, max: 1 },
    { key: "ambientLight", label: "Ambient light", min: 0, max: 1 },
  ];

  function toggleColumn(id: Column, on: boolean) {
    s.columns = on ? [...s.columns, id].sort((a, b) => allColumns.findIndex((c) => c.id === a) - allColumns.findIndex((c) => c.id === b)) : s.columns.filter((c) => c !== id);
  }

  function addCleanup() {
    const c: UserCleanup = {
      title: "New cleanup",
      command: "",
      enabled: true,
      worksForDrives: false,
      worksForFolders: true,
      worksForFiles: false,
      worksForUnc: false,
      recurse: false,
      askFirst: true,
      showConsole: true,
      wait: true,
      refresh: "item",
    };
    s.userCleanups = [...s.userCleanups, c];
    cleanupIndex = s.userCleanups.length - 1;
  }

  function removeCleanup() {
    s.userCleanups = s.userCleanups.filter((_, i) => i !== cleanupIndex);
    cleanupIndex = Math.max(0, cleanupIndex - 1);
  }

  async function saveAndClose() {
    const lines = (t: string) => t.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
    s.scan.excludeDirs = lines(excludeDirs);
    s.scan.excludeFiles = lines(excludeFiles);
    s.scan.minFileSize = Math.max(0, Math.round((Number(minSizeValue) || 0) * minSizeUnit));
    const scanChanged = JSON.stringify(s.scan) !== JSON.stringify(app.settings?.scan);
    app.dialog = null;
    await app.updateSettings($state.snapshot(s) as Settings);
    if (scanChanged && app.summary) app.notify("info", "Scanning options saved", "They apply to the next scan or refresh.");
  }

  const close = () => (app.dialog = null);
  const cleanup = $derived(s.userCleanups[cleanupIndex]);
</script>

<Dialog title="Settings" onclose={close} width={760}>
  <div class="layout">
    <nav class="pages">
      {#each tabs as t}
        <button class:on={tab === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
      {/each}
    </nav>

    <div class="page">
      {#if tab === "general"}
        <label class="check"><input type="checkbox" bind:checked={s.deleteWarning} /> Ask before moving items to the Recycle Bin</label>
        <label class="check"><input type="checkbox" bind:checked={s.showFreeSpace} /> Show free space in drive scans (F6)</label>
        <label class="check"><input type="checkbox" bind:checked={s.showUnknown} /> Show unknown space in drive scans (F7)</label>
        <label class="check"><input type="checkbox" bind:checked={s.showFileTypes} /> Show the file-type list (F8)</label>
        <label class="check"><input type="checkbox" bind:checked={s.showTreemap} /> Show the treemap (F9)</label>
        <label class="check"><input type="checkbox" bind:checked={s.showToolbar} /> Show the toolbar (F10)</label>
        <label class="check"><input type="checkbox" bind:checked={s.showStatusBar} /> Show the status bar (F11)</label>
        <label class="check"><input type="checkbox" bind:checked={s.listStripes} /> Striped rows in lists</label>
        <label class="field">
          <span class="caption">Largest files to list</span>
          <input class="input" type="number" min="10" max="10000" bind:value={s.largeFileCount} />
        </label>
        <div class="field">
          <span class="caption">Folder history ({s.folderHistory.length} remembered)</span>
          <button class="btn" disabled={!s.folderHistory.length} onclick={() => (s.folderHistory = [])}>Clear history</button>
        </div>
      {:else if tab === "columns"}
        <p class="caption">Columns shown in the folder list, besides Name, Size and Share.</p>
        {#each allColumns as c}
          <label class="check"><input type="checkbox" checked={s.columns.includes(c.id)} onchange={(e) => toggleColumn(c.id, e.currentTarget.checked)} /> {c.label}</label>
        {/each}
      {:else if tab === "treemap"}
        <div class="field">
          <span class="caption">Colors</span>
          <div class="segmented">
            <button class:on={s.view.palette === "fluent"} onclick={() => (s.view.palette = "fluent")}>Fluent</button>
            <button class:on={s.view.palette === "classic"} onclick={() => (s.view.palette = "classic")}>WinDirStat classic</button>
          </div>
        </div>
        <div class="field">
          <span class="caption">Layout</span>
          <div class="segmented">
            <button class:on={s.view.treemap.style === "KDirStat"} onclick={() => (s.view.treemap.style = "KDirStat")}>Rows (KDirStat)</button>
            <button class:on={s.view.treemap.style === "Squarified"} onclick={() => (s.view.treemap.style = "Squarified")}>Squarified (SequoiaView)</button>
          </div>
        </div>
        <div class="grid2">
          {#each sliders as sl}
            <label class="field">
              <span class="caption row">{sl.label} <span class="num">{Math.round((s.view.treemap[sl.key] as number) * 100)}%</span></span>
              <input type="range" min={sl.min} max={sl.max} step="0.01" bind:value={s.view.treemap[sl.key] as number} />
            </label>
          {/each}
          <label class="field">
            <span class="caption row">Light from left/right <span class="num">{Math.round(s.view.treemap.lightX * 100)}</span></span>
            <input type="range" min="-2" max="2" step="0.05" bind:value={s.view.treemap.lightX} />
          </label>
          <label class="field">
            <span class="caption row">Light from top/bottom <span class="num">{Math.round(s.view.treemap.lightY * 100)}</span></span>
            <input type="range" min="-2" max="2" step="0.05" bind:value={s.view.treemap.lightY} />
          </label>
        </div>
        <label class="check"><input type="checkbox" bind:checked={s.view.treemap.grid} /> Grid lines between rectangles</label>
        <div class="field"><button class="btn" onclick={() => (s.view.treemap = { ...DEFAULT_TREEMAP })}>Reset to WinDirStat defaults</button></div>
      {:else if tab === "scanning"}
        <div class="grid2">
          <label class="check"><input type="checkbox" bind:checked={s.scan.excludeHiddenDirs} /> Skip hidden folders</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.excludeHiddenFiles} /> Skip hidden files</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.excludeProtectedDirs} /> Skip protected system folders</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.excludeProtectedFiles} /> Skip protected system files</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.followJunctions} /> Follow junctions and mount points</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.followSymlinkDirs} /> Follow folder symbolic links</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.countSymlinkFiles} /> Count file symbolic links</label>
          <label class="check"><input type="checkbox" bind:checked={s.scan.useRegex} /> Patterns are regular expressions</label>
        </div>
        <div class="grid2">
          <label class="field">
            <span class="caption">Skip folders (one per line; * and ? allowed; use a full path to match one place)</span>
            <textarea class="input" rows="5" bind:value={excludeDirs} placeholder={"node_modules\nC:\\Windows\\WinSxS"}></textarea>
          </label>
          <label class="field">
            <span class="caption">Skip files</span>
            <textarea class="input" rows="5" bind:value={excludeFiles} placeholder={"*.tmp\n~$*"}></textarea>
          </label>
        </div>
        <div class="grid2">
          <label class="field">
            <span class="caption">Ignore files smaller than</span>
            <span class="inline">
              <input class="input" type="number" min="0" bind:value={minSizeValue} />
              <select class="input" bind:value={minSizeUnit}>
                <option value={1024}>KB</option>
                <option value={1048576}>MB</option>
              </select>
            </span>
          </label>
          <label class="field">
            <span class="caption">Scanning threads (0 = one per CPU)</span>
            <input class="input" type="number" min="0" max="64" bind:value={s.scan.threads} />
          </label>
        </div>
      {:else if tab === "cleanups"}
        <p class="caption">
          Your own commands, run through cmd.exe from the Clean Up menu or with Ctrl+0…9. Placeholders:
          <span class="mono">%p</span> path and <span class="mono">%n</span> name of the selected item;
          <span class="mono">%sp</span> and <span class="mono">%sn</span> for each subfolder when recursing.
        </p>
        <div class="cleanups">
          <div class="list">
            {#each s.userCleanups as c, i}
              <button class:on={i === cleanupIndex} onclick={() => (cleanupIndex = i)}>
                <span class="caption num">Ctrl+{i}</span> {c.title || "(untitled)"}
              </button>
            {/each}
            <div class="row-buttons">
              <button class="btn" onclick={addCleanup} disabled={s.userCleanups.length >= 10}><Icon name="add" size={16} /> Add</button>
              <button class="btn" onclick={removeCleanup} disabled={!cleanup}>Remove</button>
            </div>
          </div>
          {#if cleanup}
            <div class="edit">
              <label class="field"><span class="caption">Title</span><input class="input" bind:value={cleanup.title} /></label>
              <label class="field"><span class="caption">Command</span><input class="input mono" bind:value={cleanup.command} placeholder={'del /q "%sp\\*.log"'} /></label>
              <div class="grid2">
                <label class="check"><input type="checkbox" bind:checked={cleanup.enabled} /> Enabled</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.recurse} /> Also run for each subfolder</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.worksForDrives} /> Works for drives</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.worksForFolders} /> Works for folders</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.worksForFiles} /> Works for files</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.worksForUnc} /> Works on network paths</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.askFirst} /> Ask before running</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.showConsole} /> Show the console window</label>
                <label class="check"><input type="checkbox" bind:checked={cleanup.wait} /> Wait until it finishes</label>
              </div>
              <label class="field">
                <span class="caption">Afterwards, rescan</span>
                <select class="input" bind:value={cleanup.refresh}>
                  <option value="none">Nothing</option>
                  <option value="item">The selected item</option>
                  <option value="parent">Its parent folder</option>
                </select>
              </label>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <button class="btn accent" onclick={saveAndClose}>Save</button>
    <button class="btn" onclick={close}>Cancel</button>
  {/snippet}
</Dialog>

<style>
  .layout {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 20px;
    min-height: 380px;
  }
  .pages {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pages button,
  .list button {
    text-align: left;
    border: 0;
    background: transparent;
    padding: 7px 12px;
    border-radius: var(--radius-control);
    position: relative;
  }
  .pages button:hover,
  .list button:hover {
    background: var(--fill-subtle-hover);
  }
  .pages button.on,
  .list button.on {
    background: var(--fill-selected);
    font-weight: 600;
  }
  .pages button.on::before,
  .list button.on::before {
    content: "";
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .page p {
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-start;
  }
  .field .input,
  .field textarea {
    width: 100%;
  }
  textarea.input {
    resize: vertical;
    font-family: var(--mono);
    font-size: 12px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    width: 100%;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 16px;
  }
  .inline {
    display: flex;
    gap: 6px;
    width: 100%;
  }
  .inline .input:first-child {
    flex: 1;
  }
  .cleanups {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 16px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row-buttons {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .row-buttons .btn {
    min-height: 28px;
    padding: 2px 10px;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
