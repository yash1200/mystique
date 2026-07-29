<script lang="ts">
  import { Input } from "$lib/components/ui";

  interface Props {
    value?: string;
    error?: string | null;
    oncancel: () => void;
    onsubmit: (e: Event) => void;
  }

  let {
    value = $bindable(""),
    error = null,
    oncancel,
    onsubmit,
  }: Props = $props();
</script>

<form class="rounded-md border border-line bg-elevated p-2" onsubmit={onsubmit}>
  <label class="mb-1 block text-caption font-medium text-fg-secondary" for="new-db-name">
    New database
  </label>
  <Input
    id="new-db-name"
    bind:value
    placeholder="my_database"
    inputSize="sm"
    autocomplete="off"
    class="font-mono"
  />
  {#if error}
    <p class="mt-1 text-caption text-danger">{error}</p>
  {/if}
  <div class="mt-2 flex justify-end gap-1.5">
    <button
      type="button"
      class="h-control-sm rounded-md border border-line bg-transparent px-2 text-label text-fg-secondary hover:bg-hover"
      onclick={oncancel}
    >
      Cancel
    </button>
    <button
      type="submit"
      class="h-control-sm rounded-md border border-transparent bg-accent px-2 text-label font-medium text-fg-inverse hover:bg-accent-hover"
    >
      Create
    </button>
  </div>
</form>
