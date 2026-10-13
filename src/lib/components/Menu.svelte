<script lang="ts">
  import { onMount, tick } from "svelte";
  import { isEnabled, resolve, type MenuEntry, type Submenu } from "$lib/commands";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";

  let {
    entries,
    x,
    y,
    title = "",
    onclose,
    ctx,
    nested = false,
    anchorLeft = 0,
  }: {
    entries: MenuEntry[];
    x: number;
    y: number;
    title?: string;
    onclose: () => void;
    ctx?: { x: number; y: number };
    nested?: boolean;
    /** Left edge of the parent entry, for flipping a submenu leftwards. */
    anchorLeft?: number;
  } = $props();

  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  let placed = $state(false);
  let openSub = $state<{ index: number; x: number; y: number; left: number } | null>(null);
  let active = $state(-1);

  // Keep the menu inside the window once its size is known; nested menus
  // flip to the left of their parent entry when there's no room on the right.
  // Re-placed whenever its size changes (e.g. the title arrives later).
  $effect(() => {
    if (!el) return;
    const node = el;
    const place = () => {
      const r = node.getBoundingClientRect();
      let nx = x;
      let ny = y;
      if (nx + r.width > window.innerWidth - 4) nx = nested ? anchorLeft - r.width + 2 : window.innerWidth - r.width - 4;
      if (ny + r.height > window.innerHeight - 4) ny = window.innerHeight - r.height - 4;
      pos = { x: Math.max(4, nx), y: Math.max(4, ny) };
      placed = true;
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(node);
    return () => observer.disconnect();
  });

  onMount(async () => {
    await tick();
    if (!nested) el?.focus();
  });

  type Row =
    | { kind: "sep" }
    | { kind: "cmd"; id: string; label: string; icon?: string; shortcut?: string; enabled: boolean; checked?: boolean; danger?: boolean; run: () => void }
    | { kind: "sub"; sub: Submenu };

  const rows = $derived.by((): Row[] => {
    const out: Row[] = [];
    for (const e of entries) {
      if (e === "-") {
        if (out.length && out[out.length - 1].kind !== "sep") out.push({ kind: "sep" });
      } else if (typeof e === "string") {
        const c = resolve(e);
        if (!c) continue;
        out.push({
          kind: "cmd",
          id: c.id,
          label: c.label,
          icon: c.icon,
          shortcut: c.shortcut,
          enabled: isEnabled(c),
          checked: c.checked?.(),
          danger: c.danger,
          run: () => c.run(ctx),
        });
      } else {
        out.push({ kind: "sub", sub: e });
      }
    }
    while (out.length && out[out.length - 1].kind === "sep") out.pop();
    return out;
  });

  const hasChecks = $derived(rows.some((r) => r.kind === "cmd" && r.checked !== undefined));
  const hasIcons = $derived(rows.some((r) => (r.kind === "cmd" && r.icon) || (r.kind === "sub" && r.sub.icon)));

  function activate(i: number, target?: HTMLElement) {
    const row = rows[i];
    if (!row) return;
    if (row.kind === "cmd" && row.enabled) {
      onclose();
      row.run();
    } else if (row.kind === "sub") {
      const r = (target ?? el?.querySelectorAll<HTMLElement>(".entry")[i])?.getBoundingClientRect();
      if (r) openSub = { index: i, x: r.right - 2, y: r.top - 5, left: r.left };
    }
  }

  function hover(i: number, e: MouseEvent) {
    active = i;
    const row = rows[i];
    if (row?.kind === "sub") activate(i, e.currentTarget as HTMLElement);
    else openSub = null;
  }

  function onkeydown(e: KeyboardEvent) {
    const selectable = rows.map((r, i) => (r.kind !== "sep" ? i : -1)).filter((i) => i >= 0);
    const at = selectable.indexOf(active);
    if (e.key === "ArrowDown") active = selectable[(at + 1) % selectable.length];
    else if (e.key === "ArrowUp") active = selectable[(at - 1 + selectable.length) % selectable.length];
    else if (e.key === "Enter" || e.key === " " || e.key === "ArrowRight") {
      if (active >= 0 && (e.key !== "ArrowRight" || rows[active].kind === "sub")) activate(active);
    } else if (e.key === "Escape" || (e.key === "ArrowLeft" && nested)) onclose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div
  class="menu flyout"
  class:nested
  bind:this={el}
  style:left="{pos.x}px"
  style:top="{pos.y}px"
  style:visibility={placed ? "visible" : "hidden"}
  role="menu"
  tabindex="-1"
  {onkeydown}
>
  {#if title}<div class="title caption">{title}</div>{/if}
  {#each rows as row, i}
    {#if row.kind === "sep"}
      <div class="sep"></div>
    {:else if row.kind === "cmd"}
      <button
        class="entry"
        class:active={active === i}
        class:danger={row.danger}
        disabled={!row.enabled}
        role={row.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
        aria-checked={row.checked}
        onmouseenter={(e) => hover(i, e)}
        onclick={() => activate(i)}
      >
        {#if hasChecks}<span class="check">{#if row.checked}<Icon name="checkmark" size={14} />{/if}</span>{/if}
        {#if hasIcons}<span class="ico">{#if row.icon}<Icon name={row.icon as any} size={16} />{/if}</span>{/if}
        <span class="label">{row.label}</span>
        {#if row.shortcut}<span class="key">{row.shortcut}</span>{/if}
      </button>
    {:else}
      <button
        class="entry"
        class:active={active === i || openSub?.index === i}
        role="menuitem"
        aria-haspopup="menu"
        onmouseenter={(e) => hover(i, e)}
        onclick={(e) => activate(i, e.currentTarget)}
      >
        {#if hasChecks}<span class="check"></span>{/if}
        {#if hasIcons}<span class="ico">{#if row.sub.icon}<Icon name={row.sub.icon} size={16} />{/if}</span>{/if}
        <span class="label">{row.sub.label}</span>
        <span class="key"><Icon name="chevronRight" size={12} /></span>
      </button>
    {/if}
  {/each}
</div>
<!-- Outside the parent: its backdrop filter would otherwise become the
     containing block of this fixed-position submenu. -->
{#if openSub && rows[openSub.index]?.kind === "sub"}
  {@const sub = rows[openSub.index] as { kind: "sub"; sub: Submenu }}
  <Menu entries={sub.sub.items()} x={openSub.x} y={openSub.y} anchorLeft={openSub.left} nested {ctx} onclose={() => { openSub = null; onclose(); }} />
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 41;
    min-width: 230px;
    max-width: 420px;
    max-height: calc(100vh - 8px);
    overflow-y: auto;
    padding: 4px;
    outline: none;
    animation: pop 0.12s ease-out;
  }
  .menu.nested {
    z-index: 42;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
  .title {
    padding: 6px 12px 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    text-align: left;
    white-space: nowrap;
  }
  .entry.active:not(:disabled),
  .entry:hover:not(:disabled) {
    background: var(--fill-subtle-hover);
  }
  .entry:disabled {
    color: var(--text-disabled);
  }
  .entry.danger:not(:disabled) {
    color: var(--danger);
  }
  .check,
  .ico {
    width: 16px;
    display: inline-flex;
    justify-content: center;
    flex: none;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .key {
    color: var(--text-3);
    font-size: 12px;
    padding-left: 16px;
    display: inline-flex;
  }
  .sep {
    height: 1px;
    margin: 4px 0;
    background: var(--stroke-divider);
  }
</style>
