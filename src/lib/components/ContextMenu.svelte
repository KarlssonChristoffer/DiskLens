<script lang="ts">
  import { treeMenu, treemapMenu } from "$lib/commands";
  import { app } from "$lib/state.svelte";
  import Menu from "./Menu.svelte";

  let title = $state("");

  $effect(() => {
    const m = app.menu;
    title = "";
    if (!m) return;
    const ids = app.selection;
    if (ids.length > 1) title = `${ids.length} items`;
    else if (ids.length === 1) app.detail(ids[0]).then((d) => app.menu === m && (title = d.info.name));
  });

  function close() {
    app.menu = null;
  }
</script>

<svelte:window onblur={close} />

{#if app.menu}
  {@const m = app.menu}
  <div class="scrim" role="presentation" onpointerdown={close} oncontextmenu={(e) => { e.preventDefault(); close(); }}></div>
  {#key m}
    <Menu entries={m.kind === "treemap" ? treemapMenu() : treeMenu()} x={m.x} y={m.y} {title} ctx={{ x: m.x, y: m.y }} onclose={close} />
  {/key}
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
</style>
