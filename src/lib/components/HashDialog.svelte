<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { app } from "$lib/state.svelte";
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";

  const d = $derived(app.dialog?.kind === "hash" ? app.dialog : null);
  const rows = $derived(
    d?.hashes
      ? [
          ["MD5", d.hashes.md5],
          ["SHA-1", d.hashes.sha1],
          ["SHA-256", d.hashes.sha256],
          ["SHA-384", d.hashes.sha384],
          ["SHA-512", d.hashes.sha512],
        ]
      : [],
  );

  async function copy(text: string) {
    await writeText(text);
    app.notify("info", "Hash copied");
  }
</script>

{#if d}
  <Dialog title="Hashes of {d.name}" onclose={() => (app.dialog = null)} width={720}>
    {#if d.error}
      <p class="error">{d.error}</p>
    {:else if !d.hashes}
      <div class="wait"><div class="ring"></div> Reading the file…</div>
    {:else}
      <div class="hashes">
        {#each rows as [label, value]}
          <span class="caption">{label}</span>
          <span class="mono value">{value}</span>
          <button class="btn subtle icon" title="Copy" onclick={() => copy(value)}><Icon name="copy" size={16} /></button>
        {/each}
      </div>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (app.dialog = null)}>Close</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .hashes {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 6px 12px;
    align-items: center;
  }
  .value {
    word-break: break-all;
    user-select: text;
    font-size: 12px;
  }
  .wait {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .error {
    color: var(--danger);
  }
</style>
