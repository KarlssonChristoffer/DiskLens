<script lang="ts">
  import { app } from "$lib/state.svelte";
</script>

{#if app.busy}
  <div class="busy" role="status" aria-live="polite">
    <div class="panel flyout">
      <div class="ring"></div>
      <span>{app.busy}</span>
    </div>
  </div>
{/if}

<style>
  .busy {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--smoke) 50%, transparent);
    cursor: progress;
  }
  .panel {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 24px;
  }
  .ring {
    width: 24px;
    height: 24px;
  }
</style>
