<script lang="ts">
  import { menuBar } from "$lib/commands";
  import Menu from "./Menu.svelte";

  let open = $state<{ index: number; x: number; y: number } | null>(null);
  let buttons = $state<HTMLButtonElement[]>([]);

  function show(i: number) {
    const r = buttons[i].getBoundingClientRect();
    open = { index: i, x: r.left, y: r.bottom + 2 };
  }

  function toggle(i: number) {
    if (open?.index === i) open = null;
    else show(i);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      const n = menuBar.length;
      show((open.index + (e.key === "ArrowRight" ? 1 : n - 1)) % n);
      e.preventDefault();
    }
  }
</script>

<svelte:window {onkeydown} onblur={() => (open = null)} />

<nav class="menubar" aria-label="Main menu">
  {#each menuBar as m, i}
    <button
      bind:this={buttons[i]}
      class:on={open?.index === i}
      onclick={() => toggle(i)}
      onmouseenter={() => open && open.index !== i && show(i)}
    >
      {m.label}
    </button>
  {/each}
</nav>

{#if open}
  {@const m = menuBar[open.index]}
  <div class="scrim" role="presentation" onpointerdown={() => (open = null)}></div>
  {#key open.index}
    <Menu entries={m.items()} x={open.x} y={open.y} onclose={() => (open = null)} />
  {/key}
{/if}

<style>
  .menubar {
    display: flex;
    gap: 2px;
    height: 32px;
    align-items: center;
    padding: 0 8px;
  }
  button {
    border: 0;
    background: transparent;
    padding: 3px 10px;
    border-radius: var(--radius-control);
    color: var(--text-1);
  }
  button:hover,
  button.on {
    background: var(--fill-subtle-hover);
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
</style>
