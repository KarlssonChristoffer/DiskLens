<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    width = 540,
    children,
    footer,
  }: { title: string; onclose: () => void; width?: number; children: Snippet; footer?: Snippet } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="smoke" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" style:width="min({width}px, calc(100vw - 48px))" role="dialog" aria-modal="true" aria-label={title}>
    <div class="body">
      <h2>{title}</h2>
      {@render children()}
    </div>
    {#if footer}
      <div class="footer">{@render footer()}</div>
    {/if}
  </div>
</div>

<style>
  .smoke {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: var(--smoke);
    animation: fade 0.15s ease-out;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .dialog {
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-surface);
    background: var(--card-solid);
    border: 1px solid var(--stroke-surface);
    box-shadow: var(--shadow-dialog);
    overflow: hidden;
    animation: rise 0.18s cubic-bezier(0.1, 0.9, 0.2, 1);
  }
  @keyframes rise {
    from {
      transform: scale(1.04);
      opacity: 0;
    }
  }
  .body {
    padding: 24px;
    overflow: auto;
    min-height: 0;
  }
  h2 {
    margin: 0 0 16px;
    font: 600 20px/28px var(--font-display);
    word-break: break-word;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 20px 24px;
    background: var(--layer);
    border-top: 1px solid var(--stroke-divider);
  }
  .footer :global(.btn) {
    min-width: 120px;
  }
</style>
