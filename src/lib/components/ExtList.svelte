<script lang="ts">
  import { extLabel, formatBytes, formatCount, formatPercent, swatch } from "$lib/format";
  import { app } from "$lib/state.svelte";

  const total = $derived(app.extStats.reduce((s, e) => s + e.bytes, 0));
  const max = $derived(app.extStats[0]?.bytes ?? 1);

  function toggle(id: number) {
    app.highlightExt = app.highlightExt === id ? null : id;
  }
</script>

<div class="exts">
  <div class="head">
    <h2>File types</h2>
    <span class="caption num">{formatCount(app.extStats.length)}</span>
    {#if app.highlightExt !== null}
      <button class="btn subtle clear" onclick={() => (app.highlightExt = null)}>Clear highlight</button>
    {/if}
  </div>
  <div class="list">
    {#each app.extStats as e (e.id)}
      <button
        class="ext"
        class:on={app.highlightExt === e.id}
        title="{formatCount(e.files)} files · click to highlight them in the treemap"
        onclick={() => toggle(e.id)}
      >
        <span class="chip" style:background={swatch(e.color)}></span>
        <span class="main">
          <span class="line">
            <span class="label" class:none={e.ext === ""}>{extLabel(e.ext)}</span>
            <span class="num">{formatBytes(e.bytes)}</span>
          </span>
          <span class="line sub">
            <span class="bar"><span style:width="{(e.bytes / max) * 100}%" style:background={swatch(e.color)}></span></span>
            <span class="caption num">{formatPercent(total ? e.bytes / total : 0)}</span>
          </span>
        </span>
      </button>
    {/each}
  </div>
</div>

<style>
  .exts {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 10px 14px 6px;
  }
  h2 {
    margin: 0;
    font: 600 14px/20px var(--font);
  }
  .clear {
    margin-left: auto;
    min-height: 24px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--accent-text);
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 0 4px 8px;
  }
  .ext {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    background: transparent;
    text-align: left;
    border-radius: var(--radius-control);
    content-visibility: auto;
    contain-intrinsic-size: auto 46px;
  }
  .ext:hover {
    background: var(--fill-subtle-hover);
  }
  .ext.on {
    background: var(--accent-subtle);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .chip {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    flex: none;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .label {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .label.none {
    font-weight: 400;
    font-style: italic;
    color: var(--text-2);
  }
  .sub .bar {
    flex: 1;
  }
  .sub .caption {
    width: 40px;
    text-align: right;
  }
</style>
