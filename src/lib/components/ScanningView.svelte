<script lang="ts">
  import { formatBytes, formatCount, formatDuration } from "$lib/format";
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";

  const p = $derived(app.progress);
  const rate = $derived(p && p.elapsedMs > 500 ? Math.round((p.files / p.elapsedMs) * 1000) : null);
  const target = $derived(app.scanPaths.length > 1 ? `${app.scanPaths.length} drives` : app.scanPaths[0]);
</script>

<div class="scanning">
  <div class="panel card">
    {#if p?.paused}
      <div class="paused"><Icon name="pause" size={28} /></div>
    {:else}
      <div class="ring big"></div>
    {/if}
    <h1>{p?.paused ? "Paused" : "Scanning"} {target}</h1>
    <div class="stats">
      <div><span class="value num">{formatCount(p?.files ?? 0)}</span><span class="caption">files</span></div>
      <div><span class="value num">{formatCount(p?.dirs ?? 0)}</span><span class="caption">folders</span></div>
      <div><span class="value num">{formatBytes(p?.bytes ?? 0)}</span><span class="caption">found</span></div>
    </div>
    <p class="caption num">
      {formatDuration(p?.elapsedMs ?? 0)}
      {#if rate && !p?.paused}· {formatCount(rate)} files/s{/if}
      {#if p?.errors}· {formatCount(p.errors)} folders unreadable{/if}
    </p>
    <div class="buttons">
      {#if p?.paused}
        <button class="btn accent" onclick={() => app.resumeScan()}><Icon name="play" /> Resume</button>
      {:else}
        <button class="btn" onclick={() => app.pauseScan()}><Icon name="pause" /> Pause</button>
      {/if}
      <button class="btn" title="Finish now and show what's been scanned so far" onclick={() => app.stopScan()}>
        <Icon name="stop" /> Stop and show results
      </button>
      <button class="btn subtle" onclick={() => app.cancelScan()}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .scanning {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 32px;
  }
  .panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 40px 48px;
    min-width: 480px;
    max-width: 680px;
  }
  .ring.big {
    width: 48px;
    height: 48px;
    border-width: 4px;
  }
  .paused {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--accent-subtle);
    color: var(--accent-text);
  }
  h1 {
    margin: 0;
    font: 600 20px/28px var(--font-display);
    text-align: center;
    word-break: break-all;
  }
  .stats {
    display: flex;
    gap: 40px;
  }
  .stats div {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .value {
    font: 600 24px/32px var(--font-display);
  }
  p {
    margin: 0;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
</style>
