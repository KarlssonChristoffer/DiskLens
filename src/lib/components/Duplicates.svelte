<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { api, type DupeGroup, type FileEntry } from "$lib/api";
  import { formatBytes, formatCount, formatDate, formatDuration, swatch } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  let groups = $state<DupeGroup[] | null>(null);
  let searching = $state(false);
  let progress = $state<{ candidates: number; filesHashed: number; bytesHashed: number; elapsedMs: number } | null>(null);
  let minSize = $state(1024 * 1024);
  const picked = new SvelteSet<number>();

  const wasted = $derived(groups?.reduce((s, g) => s + g.wasted, 0) ?? 0);
  const pickedFiles = $derived(groups?.flatMap((g) => g.files).filter((f) => picked.has(f.info.id)) ?? []);
  const pickedBytes = $derived(pickedFiles.reduce((s, f) => s + f.info.physical, 0));

  // Pick up existing results and refresh them after deletions.
  $effect(() => {
    void app.version;
    if (!searching) load();
  });

  async function load() {
    groups = await api.getDuplicates().catch(() => null);
    const alive = new Set(groups?.flatMap((g) => g.files.map((f) => f.info.id)) ?? []);
    for (const id of picked) if (!alive.has(id)) picked.delete(id);
  }

  async function search() {
    searching = true;
    progress = null;
    picked.clear();
    try {
      await api.findDuplicates(minSize, async (e) => {
        if (e.type === "progress") progress = e;
        else {
          searching = false;
          if (e.type === "done") {
            await load();
            app.notify("success", e.groups === 0 ? "No duplicates found" : `Found ${formatCount(e.groups)} sets of duplicates`,
              e.groups ? `${formatBytes(e.wasted)} could be freed · took ${formatDuration(e.elapsedMs)}` : undefined);
          }
        }
      });
    } catch (err) {
      searching = false;
      app.notify("error", "Couldn't search for duplicates", String(err));
    }
  }

  /** Keep one file per group (newest or oldest) and pick the rest. */
  function pickAllBut(keep: "newest" | "oldest") {
    picked.clear();
    for (const g of groups ?? []) {
      const sorted = [...g.files].sort((a, b) => b.info.modified - a.info.modified);
      const kept = keep === "newest" ? sorted[0] : sorted[sorted.length - 1];
      for (const f of g.files) if (f !== kept) picked.add(f.info.id);
    }
  }

  function toggle(id: number) {
    if (picked.has(id)) picked.delete(id);
    else picked.add(id);
  }

  async function deletePicked(permanent: boolean) {
    // Refuse to remove every copy of a group by accident.
    const wipesGroup = groups?.some((g) => g.files.every((f) => picked.has(f.info.id)));
    if (wipesGroup) {
      const answer = await app.ask({
        title: "Remove every copy?",
        message: "Your selection includes all copies of at least one file, so no copy would be left.",
        items: [],
        confirmLabel: "Continue anyway",
        danger: true,
      });
      if (!answer.ok) return;
    }
    app.setSelection(pickedFiles.map((f) => f.info.id), pickedFiles[0]?.info.id ?? null, pickedFiles.map((f) => f.info));
    await app.deleteSelected(permanent);
  }

  function select(e: MouseEvent, f: FileEntry) {
    if (e.ctrlKey) app.toggleSelect(f.info.id);
    else app.select(f.info.id, f.info);
  }

  function oncontext(e: MouseEvent, f: FileEntry) {
    e.preventDefault();
    if (!app.isSelected(f.info.id)) app.select(f.info.id, f.info);
    app.menu = { x: e.clientX, y: e.clientY, kind: "tree" };
  }

  const folderOf = (path: string) => path.slice(0, path.lastIndexOf("\\") + 1);
</script>

<div class="dupes">
  {#if searching}
    <div class="center">
      <div class="ring"></div>
      <div>Comparing files…</div>
      {#if progress}
        <div class="caption num">
          {formatCount(progress.filesHashed)} checks · {formatBytes(progress.bytesHashed)} read · {formatCount(progress.candidates)} candidates · {formatDuration(progress.elapsedMs)}
        </div>
      {/if}
      <button class="btn" onclick={() => api.cancelDuplicates()}>Cancel</button>
    </div>
  {:else if groups === null}
    <div class="center intro">
      <Icon name="documentCopy" size={32} />
      <h2>Find duplicate files</h2>
      <p class="caption">
        DiskLens compares files of the same size by their content: first the opening bytes, then more, then
        everything, so most files are ruled out quickly. Online-only cloud files are skipped so nothing gets
        downloaded.
      </p>
      <div class="row">
        <label class="caption" for="min">Ignore files smaller than</label>
        <select id="min" class="input" bind:value={minSize}>
          <option value={1}>Nothing (all files)</option>
          <option value={64 * 1024}>64 KB</option>
          <option value={1024 * 1024}>1 MB</option>
          <option value={10 * 1024 * 1024}>10 MB</option>
          <option value={100 * 1024 * 1024}>100 MB</option>
        </select>
        <button class="btn accent" onclick={search} disabled={app.summary?.fromFile}>Find duplicates</button>
      </div>
    </div>
  {:else}
    <div class="toolbar">
      <span class="caption num">{formatCount(groups.length)} sets · <strong>{formatBytes(wasted)}</strong> in extra copies</span>
      <span class="spacer"></span>
      <button class="btn subtle" onclick={() => pickAllBut("newest")} disabled={!groups.length}>Keep newest</button>
      <button class="btn subtle" onclick={() => pickAllBut("oldest")} disabled={!groups.length}>Keep oldest</button>
      <button class="btn subtle" onclick={() => picked.clear()} disabled={!picked.size}>Clear</button>
      <button class="btn subtle icon" title="Search again" onclick={search}><Icon name="arrowClockwise" /></button>
      <button class="btn subtle icon" title="Delete checked permanently" disabled={!picked.size} onclick={() => deletePicked(true)}><Icon name="deleteDismiss" /></button>
      <button class="btn accent" disabled={!picked.size} onclick={() => deletePicked(false)}>
        <Icon name="delete" />
        {picked.size ? `Recycle ${formatCount(picked.size)} (${formatBytes(pickedBytes)})` : "Recycle checked"}
      </button>
    </div>
    <div class="list">
      {#each groups as g (g.hash + g.size)}
        <div class="group">
          <div class="ghead caption num">
            {g.files.length} copies · {formatBytes(g.size)} each · <strong>{formatBytes(g.wasted)} extra</strong>
          </div>
          {#each g.files as f (f.info.id)}
            <div
              class="file"
              class:selected={app.isSelected(f.info.id)}
              role="button"
              tabindex="-1"
              onclick={(e) => select(e, f)}
              onkeydown={() => {}}
              oncontextmenu={(e) => oncontext(e, f)}
            >
              <input type="checkbox" checked={picked.has(f.info.id)} onclick={(e) => { e.stopPropagation(); toggle(f.info.id); }} />
              <span class="dot" style:background={swatch(f.info.color)}></span>
              <div class="main">
                <div class="name">{f.info.name}</div>
                <div class="caption path">{folderOf(f.path)}</div>
              </div>
              <span class="caption">{formatDate(f.info.modified)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="center caption">No duplicate files found.</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dupes {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px;
    text-align: center;
  }
  .intro {
    color: var(--text-2);
  }
  .intro h2 {
    margin: 0;
    color: var(--text-1);
    font: 600 20px/28px var(--font-display);
  }
  .intro p {
    max-width: 520px;
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px 6px 12px;
    border-bottom: 1px solid var(--stroke-divider);
  }
  .toolbar strong,
  .ghead strong {
    color: var(--text-1);
  }
  .spacer {
    flex: 1;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 4px 4px 12px;
  }
  .group {
    margin-bottom: 8px;
    content-visibility: auto;
    contain-intrinsic-size: auto 140px;
  }
  .ghead {
    padding: 6px 10px 2px;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 10px;
    border-radius: var(--radius-control);
  }
  .file:hover {
    background: var(--fill-subtle-hover);
  }
  .file.selected {
    background: var(--accent-subtle);
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
</style>
