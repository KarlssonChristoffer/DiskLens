<script lang="ts">
  import { app } from "$lib/state.svelte";
  import Icon from "./Icon.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast flyout {t.kind}">
      <span class="badge">
        <Icon name={t.kind === "error" ? "warning" : t.kind === "success" ? "checkmark" : "info"} size={16} />
      </span>
      <div class="text">
        <div class="title">{t.title}</div>
        {#if t.message}<div class="caption msg">{t.message}</div>{/if}
      </div>
      <button class="btn subtle icon" aria-label="Dismiss" onclick={() => app.dismiss(t.id)}>
        <Icon name="dismiss" size={16} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 44px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(400px, calc(100vw - 32px));
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 8px 12px 16px;
    pointer-events: auto;
    animation: slide 0.2s cubic-bezier(0.1, 0.9, 0.2, 1);
  }
  @keyframes slide {
    from {
      transform: translateY(12px);
      opacity: 0;
    }
  }
  .badge {
    display: inline-grid;
    place-items: center;
    margin-top: 2px;
    color: var(--accent-text);
  }
  .success .badge {
    color: var(--success);
  }
  .error .badge {
    color: var(--danger);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .msg {
    word-break: break-word;
  }
  .btn.icon {
    width: 28px;
    min-height: 28px;
  }
</style>
