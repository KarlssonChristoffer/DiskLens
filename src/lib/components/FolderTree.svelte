<script lang="ts">
  import { tick } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { api, NodeFlags, type Column, type NodeInfo, type SortKey } from "$lib/api";
  import { formatBytes, formatCount, formatDate, formatPercent, swatch } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  const ROW = 32;
  const PAGE = 1000;
  const OVERSCAN = 8;

  interface Children {
    items: NodeInfo[];
    total: number;
  }
  type Row =
    | { kind: "node"; node: NodeInfo; depth: number }
    | { kind: "more"; parent: number; depth: number; remaining: number };

  let root = $state<NodeInfo | null>(null);
  const cache = new SvelteMap<number, Children>();
  const expanded = new SvelteSet<number>([0]);
  const owners = new SvelteMap<number, string | null>();
  let sort = $state<SortKey>("size");
  let descending = $state(true);
  let anchor: number | null = null;

  let viewport: HTMLDivElement;
  let scrollTop = $state(0);
  let viewHeight = $state(0);

  const COLUMN_DEFS: Record<Column, { label: string; width: string; sort?: SortKey; align?: "right" }> = {
    subtree: { label: "Share", width: "128px" },
    physical: { label: "On disk", width: "84px", sort: "physical", align: "right" },
    logical: { label: "File size", width: "84px", sort: "logical", align: "right" },
    items: { label: "Items", width: "72px", sort: "items", align: "right" },
    files: { label: "Files", width: "72px", sort: "files", align: "right" },
    folders: { label: "Folders", width: "72px", sort: "folders", align: "right" },
    modified: { label: "Last change", width: "142px", sort: "modified" },
    attributes: { label: "Attr.", width: "56px", sort: "attributes" },
    owner: { label: "Owner", width: "150px" },
  };
  // The share column is always shown, as in WinDirStat; the rest are optional.
  const columns = $derived<Column[]>(["subtree", ...((app.settings?.columns ?? []) as Column[]).filter((c) => c !== "subtree")]);
  const template = $derived(`minmax(200px, 1fr) 88px ${columns.map((c) => COLUMN_DEFS[c].width).join(" ")}`);
  const minWidth = $derived(200 + 88 + columns.reduce((s, c) => s + parseInt(COLUMN_DEFS[c].width), 0) + 16);

  const rows = $derived.by(() => {
    const out: Row[] = [];
    if (!root) return out;
    const walk = (node: NodeInfo, depth: number) => {
      out.push({ kind: "node", node, depth });
      if (!node.isDir || !expanded.has(node.id)) return;
      const c = cache.get(node.id);
      if (!c) return;
      for (const child of c.items) walk(child, depth + 1);
      if (c.items.length < c.total) {
        out.push({ kind: "more", parent: node.id, depth: depth + 1, remaining: c.total - c.items.length });
      }
    };
    walk(root, 0);
    return out;
  });

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / ROW) + OVERSCAN));
  const visibleIds = $derived(rows.slice(first, last).flatMap((r) => (r.kind === "node" ? [r.node.id] : [])));

  // Owners are slow to look up; fetch them only for visible rows.
  let ownerTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (!columns.includes("owner")) return;
    const ids = visibleIds.filter((id) => !owners.has(id));
    if (!ids.length) return;
    clearTimeout(ownerTimer);
    ownerTimer = setTimeout(async () => {
      const list = await api.owners(ids).catch(() => ids.map(() => null));
      ids.forEach((id, i) => owners.set(id, list[i]));
    }, 150);
  });

  async function loadChildren(id: number, append = false) {
    const existing = cache.get(id);
    const offset = append && existing ? existing.items.length : 0;
    const page = await api.listChildren(id, sort, descending, offset, PAGE);
    cache.set(id, {
      items: append && existing ? [...existing.items, ...page.items] : page.items,
      total: page.total,
    });
  }

  // Reload whenever the data or the sort order changes, keeping what's open.
  $effect(() => {
    void [app.version, sort, descending];
    reload();
  });

  async function reload() {
    owners.clear();
    let info: NodeInfo;
    try {
      info = (await app.detail(0)).info;
    } catch {
      return;
    }
    const fresh = new Map<number, Children>();
    await Promise.all(
      [...expanded].map(async (id) => {
        try {
          const keep = Math.max(PAGE, cache.get(id)?.items.length ?? 0);
          const page = await api.listChildren(id, sort, descending, 0, keep);
          fresh.set(id, { items: page.items, total: page.total });
        } catch {
          expanded.delete(id);
        }
      }),
    );
    cache.clear();
    for (const [k, v] of fresh) cache.set(k, v);
    root = info;
  }

  async function toggle(node: NodeInfo) {
    if (expanded.has(node.id)) expanded.delete(node.id);
    else {
      expanded.add(node.id);
      if (!cache.has(node.id)) await loadChildren(node.id);
    }
  }

  // Expand / collapse requested from the context menu.
  $effect(() => {
    const req = app.toggleRequest;
    if (!req) return;
    app.toggleRequest = null;
    const row = rows.find((r) => r.kind === "node" && r.node.id === req.id);
    if (row?.kind === "node") toggle(row.node);
  });

  // Reveal the focused item when it changes elsewhere (treemap, lists).
  $effect(() => {
    const id = app.focus;
    if (id !== null) reveal(id);
  });

  async function reveal(id: number) {
    const d = await app.detail(id).catch(() => null);
    if (!d || !d.alive) return;
    const chain = d.ancestry;
    for (let i = 0; i < chain.length - 1; i++) {
      const parent = chain[i];
      const next = chain[i + 1];
      expanded.add(parent);
      if (!cache.has(parent)) await loadChildren(parent);
      let c = cache.get(parent);
      while (c && !c.items.some((n) => n.id === next) && c.items.length < c.total) {
        await loadChildren(parent, true);
        c = cache.get(parent);
      }
    }
    await tick();
    const index = rows.findIndex((r) => r.kind === "node" && r.node.id === id);
    if (index < 0 || !viewport) return;
    const top = index * ROW;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + ROW > viewport.scrollTop + viewHeight) viewport.scrollTop = top + ROW - viewHeight;
  }

  function setSort(key: SortKey) {
    if (sort === key) descending = !descending;
    else {
      sort = key;
      descending = key !== "name" && key !== "attributes";
    }
  }

  const nodeRows = () => rows.flatMap((r) => (r.kind === "node" ? [r.node] : []));

  function click(e: MouseEvent, node: NodeInfo) {
    if (e.ctrlKey) {
      app.toggleSelect(node.id);
      anchor = node.id;
    } else if (e.shiftKey && anchor !== null) {
      const list = nodeRows();
      const a = list.findIndex((n) => n.id === anchor);
      const b = list.findIndex((n) => n.id === node.id);
      if (a >= 0 && b >= 0) {
        const range = list.slice(Math.min(a, b), Math.max(a, b) + 1);
        app.setSelection(range.map((n) => n.id), node.id, range);
      }
    } else {
      app.select(node.id, node);
      anchor = node.id;
    }
  }

  function oncontext(e: MouseEvent, node: NodeInfo) {
    e.preventDefault();
    if (!app.isSelected(node.id)) {
      app.select(node.id, node);
      anchor = node.id;
    }
    app.menu = { x: e.clientX, y: e.clientY, kind: "tree" };
  }

  function onkeydown(e: KeyboardEvent) {
    const list = rows;
    const i = list.findIndex((r) => r.kind === "node" && r.node.id === app.focus);
    const row = list[i];
    const node = row?.kind === "node" ? row.node : null;
    const pick = (j: number) => {
      const r = list[Math.max(0, Math.min(list.length - 1, j))];
      if (r?.kind !== "node") return;
      if (e.shiftKey && anchor !== null) {
        const nodes = nodeRows();
        const a = nodes.findIndex((n) => n.id === anchor);
        const b = nodes.findIndex((n) => n.id === r.node.id);
        const range = nodes.slice(Math.min(a, b), Math.max(a, b) + 1);
        app.setSelection(range.map((n) => n.id), r.node.id, range);
      } else {
        app.select(r.node.id, r.node);
        anchor = r.node.id;
      }
    };
    switch (e.key) {
      case "ArrowDown":
        pick(i < 0 ? 0 : i + 1);
        break;
      case "ArrowUp":
        pick(i - 1);
        break;
      case "PageDown":
        pick(i + Math.floor(viewHeight / ROW));
        break;
      case "PageUp":
        pick(i - Math.floor(viewHeight / ROW));
        break;
      case "Home":
        pick(0);
        break;
      case "End":
        pick(list.length - 1);
        break;
      case "ArrowRight":
        if (node?.isDir && node.hasChildren) {
          if (!expanded.has(node.id)) toggle(node);
          else pick(i + 1);
        }
        break;
      case "ArrowLeft":
        if (node && expanded.has(node.id) && node.isDir) toggle(node);
        else if (row && row.depth > 0) {
          for (let j = i - 1; j >= 0; j--) {
            if (list[j].depth === row.depth - 1) {
              pick(j);
              break;
            }
          }
        }
        break;
      default:
        return; // other keys go to the global shortcuts
    }
    e.preventDefault();
    e.stopPropagation();
  }

  function cell(node: NodeInfo, c: Column): string {
    switch (c) {
      case "physical":
        return formatBytes(node.physical);
      case "logical":
        return formatBytes(node.logical);
      case "items":
        return node.isDir ? formatCount(node.files + node.dirs) : "";
      case "files":
        return node.isDir ? formatCount(node.files) : "";
      case "folders":
        return node.isDir ? formatCount(node.dirs) : "";
      case "modified":
        return formatDate(node.modified);
      case "attributes":
        return node.attributeText;
      case "owner":
        return owners.get(node.id) ?? "";
      default:
        return "";
    }
  }

  const iconFor = (node: NodeInfo) =>
    node.flags & NodeFlags.myComputer ? "storage" : node.flags & NodeFlags.drive ? "hardDrive" : node.kind === "free" ? "storage" : node.kind === "unknown" ? "questionCircle" : null;
</script>

<div class="tree" class:stripes={app.settings?.listStripes}>
  <div class="scroller">
    <div class="header row" style:grid-template-columns={template} style:min-width="{minWidth}px">
      <button class:active={sort === "name"} onclick={() => setSort("name")}>
        Name {#if sort === "name"}<span class="arrow">{descending ? "▾" : "▴"}</span>{/if}
      </button>
      <button class="right" class:active={sort === "size"} onclick={() => setSort("size")}>
        Size {#if sort === "size"}<span class="arrow">{descending ? "▾" : "▴"}</span>{/if}
      </button>
      {#each columns as c}
        {@const def = COLUMN_DEFS[c]}
        {#if def.sort}
          <button class:right={def.align === "right"} class:active={sort === def.sort} onclick={() => setSort(def.sort!)}>
            {def.label} {#if sort === def.sort}<span class="arrow">{descending ? "▾" : "▴"}</span>{/if}
          </button>
        {:else}
          <span>{def.label}</span>
        {/if}
      {/each}
    </div>

    <div
      class="viewport"
      bind:this={viewport}
      bind:clientHeight={viewHeight}
      onscroll={() => (scrollTop = viewport.scrollTop)}
      tabindex="0"
      role="tree"
      aria-multiselectable="true"
      style:min-width="{minWidth}px"
      {onkeydown}
    >
      <div class="spacer" style:height="{rows.length * ROW}px">
        {#each rows.slice(first, last) as row, i (row.kind === "node" ? row.node.id : `more-${row.parent}`)}
          {@const top = (first + i) * ROW}
          {#if row.kind === "node"}
            {@const node = row.node}
            {@const icon = iconFor(node)}
            <div
              class="row item"
              class:odd={(first + i) % 2 === 1}
              class:selected={app.isSelected(node.id)}
              class:focused={app.focus === node.id}
              class:gone={node.flags & NodeFlags.incomplete}
              style:transform="translateY({top}px)"
              style:grid-template-columns={template}
              role="treeitem"
              aria-selected={app.isSelected(node.id)}
              aria-expanded={node.isDir ? expanded.has(node.id) : undefined}
              tabindex="-1"
              onclick={(e) => click(e, node)}
              ondblclick={() => (node.isDir && node.hasChildren ? toggle(node) : null)}
              onkeydown={() => {}}
              oncontextmenu={(e) => oncontext(e, node)}
            >
              <div class="name" style:padding-left="{4 + row.depth * 16}px">
                {#if node.isDir && node.hasChildren}
                  <button class="chev" tabindex="-1" onclick={(e) => { e.stopPropagation(); toggle(node); }}>
                    <Icon name={expanded.has(node.id) ? "chevronDown" : "chevronRight"} size={12} />
                  </button>
                {:else}
                  <span class="chev"></span>
                {/if}
                {#if icon}
                  <span class="glyph {node.kind}"><Icon name={icon} size={16} /></span>
                {:else if node.isDir}
                  <span class="folder"><Icon name="folder" size={16} /></span>
                {:else}
                  <span class="dot" style:background={swatch(node.color)}></span>
                {/if}
                <span class="label" class:pseudo={node.kind === "free" || node.kind === "unknown"}>{node.name}</span>
                {#if node.flags & NodeFlags.inaccessible}
                  <span class="warn" title="DiskLens couldn't read this folder"><Icon name="warning" size={14} /></span>
                {/if}
                {#if node.flags & NodeFlags.incomplete}
                  <span class="tag caption" title="The scan was stopped before this folder was finished">incomplete</span>
                {/if}
                {#if node.flags & NodeFlags.reparseSkipped}
                  <span class="tag caption" title="A link to another folder; not followed">link</span>
                {/if}
              </div>
              <div class="right num strong">{formatBytes(node.size)}</div>
              {#each columns as c}
                {#if c === "subtree"}
                  <div class="share">
                    <div class="bar"><span style:width="{node.share * 100}%"></span></div>
                    <span class="caption num">{formatPercent(node.share)}</span>
                  </div>
                {:else}
                  <div class="cell caption num" class:right={COLUMN_DEFS[c].align === "right"}>{cell(node, c)}</div>
                {/if}
              {/each}
            </div>
          {:else}
            <div class="row more" style:transform="translateY({top}px)">
              <div class="name" style:padding-left="{4 + row.depth * 16 + 20}px">
                <button class="btn subtle" onclick={() => loadChildren(row.parent, true)}>
                  Show {formatCount(Math.min(PAGE, row.remaining))} more of {formatCount(row.remaining)}
                </button>
              </div>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .tree {
    height: 100%;
    overflow: hidden;
  }
  .scroller {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .row {
    display: grid;
    align-items: center;
    height: 32px;
    padding-right: 8px;
  }
  .header {
    flex: none;
    border-bottom: 1px solid var(--stroke-divider);
  }
  .header > * {
    border: 0;
    background: none;
    padding: 0 8px;
    text-align: left;
    font-size: 12px;
    color: var(--text-2);
    height: 100%;
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .header button:hover {
    color: var(--text-1);
    background: var(--fill-subtle-hover);
  }
  .header .active {
    color: var(--text-1);
    font-weight: 600;
  }
  .header .right {
    justify-content: flex-end;
  }
  .arrow {
    font-size: 10px;
  }
  .viewport {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
    position: relative;
  }
  .spacer {
    position: relative;
  }
  .item,
  .more {
    position: absolute;
    left: 4px;
    right: 4px;
    border-radius: var(--radius-control);
  }
  .stripes .item.odd {
    background: var(--fill-subtle-pressed);
  }
  .item:hover {
    background: var(--fill-subtle-hover);
  }
  .item.selected,
  .stripes .item.selected {
    background: var(--accent-subtle);
  }
  .item.focused::before {
    content: "";
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .viewport:focus-visible .item.focused {
    outline: 1px solid var(--text-1);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding-right: 8px;
  }
  .chev {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    padding: 0;
    background: none;
    border-radius: var(--radius-control);
    color: var(--text-2);
    flex: none;
  }
  button.chev:hover {
    background: var(--fill-subtle-hover);
  }
  .folder {
    color: #e8a73a;
    display: inline-flex;
  }
  .glyph {
    display: inline-flex;
    color: var(--text-2);
  }
  .glyph.unknown {
    color: #d8b400;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 3px;
    margin: 0 2px;
    flex: none;
  }
  .label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .label.pseudo {
    font-style: italic;
    color: var(--text-2);
  }
  .warn {
    color: #d8a100;
    display: inline-flex;
  }
  .tag {
    padding: 0 6px;
    border-radius: 10px;
    background: var(--fill-subtle-hover);
    flex: none;
  }
  .right {
    text-align: right;
    justify-content: flex-end;
  }
  .strong {
    padding: 0 8px;
  }
  .cell {
    padding: 0 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .share {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px;
  }
  .share .bar {
    flex: 1;
  }
  .share .caption {
    width: 38px;
    text-align: right;
  }
  .more .btn {
    min-height: 26px;
    padding: 2px 8px;
    color: var(--accent-text);
  }
</style>
