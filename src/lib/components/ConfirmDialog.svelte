<script lang="ts">
  import { app } from "$lib/state.svelte";

  let dontAsk = $state(false);

  function answer(ok: boolean) {
    const req = app.confirm;
    app.confirm = null;
    req?.resolve(ok, ok && dontAsk);
    dontAsk = false;
  }
</script>

<svelte:window onkeydown={(e) => app.confirm && e.key === "Escape" && answer(false)} />

{#if app.confirm}
  {@const req = app.confirm}
  <div class="smoke" role="presentation">
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title">
      <div class="body">
        <h2 id="confirm-title">{req.title}</h2>
        <p>{req.message}</p>
        {#if req.items.length}
          <ul class="items caption">
            {#each req.items.slice(0, 50) as item}
              <li>{item}</li>
            {/each}
            {#if req.items.length > 50}<li>…and {req.items.length - 50} more</li>{/if}
          </ul>
        {/if}
        {#if req.offerDontAsk}
          <label class="check"><input type="checkbox" bind:checked={dontAsk} /> Don't ask again</label>
        {/if}
      </div>
      <div class="footer">
        <button class="btn" class:danger={req.danger} class:accent={!req.danger} onclick={() => answer(true)}>
          {req.confirmLabel}
        </button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn" autofocus onclick={() => answer(false)}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .smoke {
    position: fixed;
    inset: 0;
    z-index: 55;
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
    width: min(560px, calc(100vw - 48px));
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
  }
  h2 {
    margin: 0 0 12px;
    font: 600 20px/28px var(--font-display);
    word-break: break-word;
  }
  p {
    margin: 0;
  }
  .items {
    margin: 12px 0 0;
    padding: 8px 12px 8px 28px;
    max-height: 180px;
    overflow: auto;
    border-radius: var(--radius-control);
    background: var(--fill-subtle-hover);
    word-break: break-all;
    user-select: text;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
  }
  .footer {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 24px;
    background: var(--layer);
    border-top: 1px solid var(--stroke-divider);
  }
</style>
