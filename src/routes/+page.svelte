<script lang="ts">
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { commandForShortcut, isEnabled, shortcutOf } from "$lib/commands";
  import { app } from "$lib/state.svelte";
  import AboutDialog from "$lib/components/AboutDialog.svelte";
  import BusyOverlay from "$lib/components/BusyOverlay.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import ContextMenu from "$lib/components/ContextMenu.svelte";
  import HashDialog from "$lib/components/HashDialog.svelte";
  import MenuBar from "$lib/components/MenuBar.svelte";
  import ResultsView from "$lib/components/ResultsView.svelte";
  import ScanningView from "$lib/components/ScanningView.svelte";
  import SearchDialog from "$lib/components/SearchDialog.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import StartView from "$lib/components/StartView.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import Toolbar from "$lib/components/Toolbar.svelte";

  onMount(() => {
    app.init();
  });

  // Replace the browser's own context menu everywhere except text fields.
  function oncontextmenu(e: MouseEvent) {
    const t = e.target as HTMLElement;
    if (!t.closest("input, textarea")) e.preventDefault();
  }

  // WinDirStat's keyboard accelerators, unless typing or a dialog is open.
  function onkeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || app.confirm || app.dialog || app.menu || app.busy) return;
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, select, [contenteditable]")) return;
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
    const shortcut = shortcutOf(e);
    // Block browser reload/print/find shortcuts that would break the app.
    if (["F5", "Ctrl+R", "Ctrl+P", "Ctrl+F", "Ctrl+S", "Ctrl+O", "Ctrl+L", "Ctrl+H", "Ctrl+E", "Ctrl+M", "Ctrl+I"].includes(shortcut)) e.preventDefault();
    const c = commandForShortcut(shortcut);
    if (!c) return;
    e.preventDefault();
    if (isEnabled(c)) c.run();
  }
</script>

<svelte:window {oncontextmenu} {onkeydown} />

<main>
  <MenuBar />
  {#if app.settings?.showToolbar !== false}
    <Toolbar />
  {/if}
  <div class="content">
    {#if app.phase === "start"}
      <StartView />
    {:else if app.phase === "scanning"}
      <ScanningView />
    {:else}
      <ResultsView />
    {/if}
  </div>
  {#if app.phase === "results" && app.settings?.showStatusBar !== false}
    <StatusBar />
  {/if}
</main>

{#if app.dialog?.kind === "settings" && app.settings}
  <SettingsDialog page={app.dialog.page} />
{:else if app.dialog?.kind === "hash"}
  <HashDialog />
{:else if app.dialog?.kind === "search"}
  <SearchDialog />
{:else if app.dialog?.kind === "about"}
  <AboutDialog />
{/if}

<ContextMenu />
<BusyOverlay />
<ConfirmDialog />
<Toasts />

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
</style>
