<script lang="ts">
  import { app } from "$lib/state.svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
</script>

<Dialog title="About DiskLens" onclose={() => (app.dialog = null)} width={500}>
  <div class="about">
    <div class="logo"><Icon name="dataTreemap" size={32} /></div>
    <div>
      <div class="name">DiskLens {app.info?.version}</div>
      <p class="caption">
        A disk usage analyzer for Windows 11. Its scanner, cushion treemap, file-type colours and
        duplicate search follow WinDirStat 2.5.0, studied with REA (Reverse Engineer Anything) and
        cross-checked with WinDirStat's GPLv3 source.
      </p>
      <p class="caption">{app.info?.elevated ? "Running as administrator." : "Running without administrator rights."}</p>
    </div>
  </div>
  <h3>Keyboard shortcuts</h3>
  <div class="keys caption">
    <span>Del / Shift+Del</span><span>Recycle / delete permanently</span>
    <span>Enter / Alt+Enter</span><span>Open / Properties</span>
    <span>Ctrl+C, Ctrl+E</span><span>Copy path, show in Explorer</span>
    <span>Ctrl+P, Ctrl+S</span><span>Command Prompt, PowerShell</span>
    <span>F5 / Shift+F5</span><span>Refresh selected / all</span>
    <span>+ / − / * / /</span><span>Zoom in, out, reselect child, select parent</span>
    <span>F6–F11</span><span>Free space, unknown, file types, treemap, toolbar, status bar</span>
    <span>Ctrl+F, Ctrl+H</span><span>Search, compute hash</span>
    <span>Ctrl+0…9</span><span>User-defined cleanups</span>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={() => (app.dialog = null)}>Close</button>
  {/snippet}
</Dialog>

<style>
  .about {
    display: flex;
    gap: 16px;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    flex: none;
    border-radius: 12px;
    background: var(--accent);
    color: var(--on-accent);
  }
  .name {
    font: 600 18px/24px var(--font-display);
  }
  p {
    margin: 6px 0 0;
  }
  h3 {
    margin: 20px 0 8px;
    font: 600 14px/20px var(--font);
  }
  .keys {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 16px;
  }
  .keys span:nth-child(odd) {
    color: var(--text-1);
    font-family: var(--mono);
    font-size: 12px;
  }
</style>
