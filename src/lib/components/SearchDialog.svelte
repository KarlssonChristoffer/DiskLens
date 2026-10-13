<script lang="ts">
  import { app } from "$lib/state.svelte";
  import Dialog from "./Dialog.svelte";

  let query = $state(app.searchQuery);
  let scope = $state("");

  $effect(() => {
    app.detail(app.zoomRoot).then((d) => (scope = d.path));
  });

  function search(e: Event) {
    e.preventDefault();
    if (query.trim()) app.runSearch(query.trim());
  }
</script>

{#if app.settings}
  <Dialog title="Search" onclose={() => (app.dialog = null)} width={480}>
    <form id="search-form" onsubmit={search}>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" autofocus bind:value={query} placeholder="File or folder name" />
      <p class="caption">Searching in {scope}</p>
      <label class="check"><input type="checkbox" bind:checked={app.settings.search.caseSensitive} /> Match case</label>
      <label class="check"><input type="checkbox" bind:checked={app.settings.search.wholeName} /> Match the whole name</label>
      <label class="check"><input type="checkbox" bind:checked={app.settings.search.regex} /> Regular expression</label>
      <label class="field">
        <span class="caption">Maximum results</span>
        <input class="input" type="number" min="1" max="1000000" bind:value={app.settings.search.maxResults} />
      </label>
    </form>
    {#snippet footer()}
      <button class="btn accent" type="submit" form="search-form" disabled={!query.trim()}>Search</button>
      <button class="btn" onclick={() => (app.dialog = null)}>Cancel</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  p {
    margin: 0;
    word-break: break-all;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
</style>
