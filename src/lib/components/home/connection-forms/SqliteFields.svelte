<script lang="ts">
  import { Button, Card, Checkbox, Field, Input } from "$lib/components/ui";
  import {
    isLikelySqlitePath,
    pickSqliteDatabasePath,
  } from "$lib/utils/pickSqliteFile";

  interface Props {
    path?: string;
    readOnly?: boolean;
  }

  let {
    path = $bindable(""),
    readOnly = $bindable(true),
  }: Props = $props();

  let picking = $state(false);
  let pickError = $state<string | null>(null);

  const pathHint = $derived.by(() => {
    if (path.trim() && !isLikelySqlitePath(path)) {
      return "Unusual extension — usually .sqlite / .db (any valid SQLite file still works). Use Browse… to pick a file.";
    }
    return "Use Browse… or paste an absolute path to a .db / .sqlite file.";
  });

  async function onBrowse() {
    pickError = null;
    picking = true;
    try {
      const result = await pickSqliteDatabasePath(path);
      if (result.ok) {
        path = result.path;
        return;
      }
      if (!result.cancelled) {
        pickError = result.error;
      }
    } finally {
      picking = false;
    }
  }
</script>

<Card title="Database file">
  <Field
    label="File path"
    hint={pickError ? undefined : pathHint}
    error={pickError ?? undefined}
    full
  >
    <div class="flex min-w-0 items-center gap-2">
      <Input
        bind:value={path}
        placeholder="/path/to/database.sqlite"
        autocomplete="off"
        class="min-w-0 flex-1 font-mono"
        aria-label="SQLite database file path"
        oninput={() => {
          pickError = null;
        }}
      />
      <Button
        type="button"
        variant="secondary"
        disabled={picking}
        onclick={onBrowse}
        class="shrink-0"
        aria-label="Browse for SQLite database file"
      >
        {picking ? "Opening…" : "Browse…"}
      </Button>
    </div>
  </Field>
  <div class="mt-3">
    <Checkbox bind:checked={readOnly} label="Read-only (recommended)" />
  </div>
</Card>
