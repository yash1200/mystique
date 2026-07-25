<script lang="ts">
  import {
    CONNECTION_COLORS,
    ENGINE_OPTIONS,
    draftFromProfile,
    type ConnectionFormDraft,
    type ConnectionProfile,
    type ConnectionColorId,
    type EngineId,
    type Environment,
  } from "$lib/types/connection";
  import {
    Alert,
    Button,
    Card,
    Checkbox,
    ColorSwatchGroup,
    ContentPanel,
    Field,
    FormGrid,
    Inline,
    Input,
    PageHeader,
    Select,
    Stack,
    Textarea,
  } from "$lib/components/ui";

  interface Props {
    /** When set, form is in edit mode and fields are prefilled. */
    initial?: ConnectionProfile | null;
    saving?: boolean;
    oncancel?: () => void;
    onsave?: (draft: ConnectionFormDraft) => void | Promise<void>;
    ontest?: () => void;
  }

  let {
    initial = null,
    saving = false,
    oncancel,
    onsave,
    ontest,
  }: Props = $props();

  let name = $state("");
  let engine = $state<EngineId>("postgres");
  let colorId = $state<ConnectionColorId>("teal");
  let host = $state("127.0.0.1");
  let port = $state(5432);
  let database = $state("");
  let username = $state("");
  let password = $state("");
  let ssl = $state(false);
  let readOnly = $state(true);
  let environment = $state<Environment>("local");
  let notes = $state("");
  let formError = $state<string | null>(null);
  let skipEnginePortEffect = $state(false);

  const isEdit = $derived(!!initial?.id);

  const selectedEngine = $derived(ENGINE_OPTIONS.find((e) => e.id === engine));
  const colorHexVal = $derived(
    CONNECTION_COLORS.find((c) => c.id === colorId)?.hex ?? "#4ecdc4",
  );

  const engineHint = $derived(
    selectedEngine
      ? `${selectedEngine.description} · ${selectedEngine.queryLanguage}`
      : undefined,
  );

  // Prefill when opening edit; reset defaults when opening create.
  $effect(() => {
    skipEnginePortEffect = true;
    formError = null;
    if (initial) {
      const d = draftFromProfile(initial);
      name = d.name;
      engine = d.engine;
      colorId = d.colorId;
      host = d.host;
      port = d.port;
      database = d.database;
      username = d.username;
      password = "";
      ssl = d.ssl;
      readOnly = d.readOnly;
      environment = d.environment;
      notes = d.notes;
    } else {
      name = "";
      engine = "postgres";
      colorId = "teal";
      host = "127.0.0.1";
      port = 5432;
      database = "";
      username = "";
      password = "";
      ssl = false;
      readOnly = true;
      environment = "local";
      notes = "";
    }
    queueMicrotask(() => {
      skipEnginePortEffect = false;
    });
  });

  $effect(() => {
    if (skipEnginePortEffect) return;
    if (engine === "postgres") port = 5432;
    else if (engine === "mysql") port = 3306;
    else if (engine === "mongodb") port = 27017;
    else if (engine === "sqlite") port = 0;
  });

  async function submit(e: Event) {
    e.preventDefault();
    formError = null;
    if (!name.trim()) {
      formError = "Give this connection a name.";
      return;
    }
    if (!selectedEngine?.available && !isEdit) {
      formError = "Selected engine is not available yet.";
      return;
    }
    if (engine === "sqlite") {
      // path engines later — for now require host/db for relational
    } else if (!host.trim() || !database.trim()) {
      formError = "Host and database are required.";
      return;
    }
    try {
      await onsave?.({
        name: name.trim(),
        engine,
        colorId,
        host: host.trim(),
        port: Number(port) || 5432,
        database: database.trim(),
        username: username.trim(),
        password,
        ssl,
        readOnly,
        environment,
        notes: notes.trim(),
      });
    } catch (err) {
      formError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<ContentPanel size="md" ariaLabel={isEdit ? "Edit connection" : "Create connection"}>
  <PageHeader
    title={isEdit ? "Edit connection" : "New connection"}
    description={isEdit
      ? "Update connection details. Leave password blank to keep the existing secret reference."
      : "Choose an engine, then fill connection details. Profiles are saved to the local app database."}
  />

  <form class="m-0" onsubmit={submit}>
    <Stack gap="md">
      <FormGrid>
        <Field label="Database engine" hint={engineHint}>
          <Select bind:value={engine} disabled={isEdit}>
            {#each ENGINE_OPTIONS as eng (eng.id)}
              <option value={eng.id} disabled={!eng.available && eng.id !== engine}>
                {eng.name}{eng.available ? "" : " (coming soon)"}
              </option>
            {/each}
          </Select>
        </Field>

        <Field label="Environment">
          <Select bind:value={environment}>
            <option value="local">Local</option>
            <option value="staging">Staging</option>
            <option value="production">Production</option>
          </Select>
        </Field>
      </FormGrid>

      <Field label="Display name">
        <Input
          placeholder="e.g. local-pg"
          bind:value={name}
          autocomplete="off"
        />
      </Field>

      <Field label="Connection color">
        <ColorSwatchGroup colors={CONNECTION_COLORS} bind:value={colorId} />
        <div
          class="mt-1 inline-flex items-center gap-2 self-start rounded-md border border-line conn-tint-soft py-1.5 pr-2.5 pl-0"
          style="--connection-color: {colorHexVal}"
        >
          <span class="h-5 w-stripe rounded-r-sm bg-conn"></span>
          <span class="pr-2 text-label font-medium text-fg">
            {name.trim() || "connection-name"}
          </span>
        </div>
      </Field>

      <Card title="Server">
        <FormGrid>
          <Field label="Host">
            <Input bind:value={host} autocomplete="off" />
          </Field>
          <Field label="Port">
            <Input type="number" bind:value={port} min="1" max="65535" />
          </Field>
          <Field label="Database">
            <Input bind:value={database} autocomplete="off" />
          </Field>
          <Field label="Username">
            <Input bind:value={username} autocomplete="username" />
          </Field>
          <Field
            label="Password"
            hint={isEdit && initial?.passwordSet
              ? "Leave blank to keep the existing secret reference."
              : "Keychain storage comes next; a secret reference is saved when set."}
            full
          >
            <Input
              type="password"
              bind:value={password}
              autocomplete="new-password"
              placeholder={isEdit && initial?.passwordSet ? "••••••••" : "Optional"}
            />
          </Field>
        </FormGrid>

        <div class="mt-3 flex flex-wrap gap-4">
          <Checkbox bind:checked={ssl} label="Require SSL" />
          <Checkbox bind:checked={readOnly} label="Read-only (recommended)" />
        </div>
      </Card>

      <Field label="Notes">
        <Textarea
          rows={2}
          bind:value={notes}
          placeholder="Optional context for your team"
        />
      </Field>

      {#if formError}
        <Alert tone="danger">{formError}</Alert>
      {/if}

      <Inline gap="sm" justify="end">
        {#if oncancel}
          <Button variant="ghost" onclick={oncancel} disabled={saving}>Cancel</Button>
        {/if}
        <Button variant="ghost" onclick={ontest} disabled={saving}>Test connection</Button>
        <Button type="submit" variant="primary" disabled={saving}>
          {saving ? "Saving…" : isEdit ? "Update connection" : "Save connection"}
        </Button>
      </Inline>
    </Stack>
  </form>
</ContentPanel>
