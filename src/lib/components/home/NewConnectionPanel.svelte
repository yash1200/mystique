<script lang="ts">
  import {
    CONNECTION_COLORS,
    ENGINE_OPTIONS,
    emptyDraft,
    draftFromProfile,
    type ConnectionFormDraft,
    type ConnectionProfile,
    type ConnectionColorId,
    type EngineId,
    type Environment,
  } from "$lib/types/connection";
  import {
    Alert,
    Badge,
    Button,
    Card,
    Checkbox,
    ColorSwatchGroup,
    ContentPanel,
    EngineBadge,
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
    /** Engine chosen on the picker (create flow). Locks the engine field. */
    lockedEngine?: EngineId | null;
    saving?: boolean;
    oncancel?: () => void;
    /** Return to engine picker (create flow only). */
    onback?: () => void;
    onsave?: (draft: ConnectionFormDraft) => void | Promise<void>;
    ontest?: () => void;
  }

  let {
    initial = null,
    lockedEngine = null,
    saving = false,
    oncancel,
    onback,
    onsave,
    ontest,
  }: Props = $props();

  let name = $state("");
  let engine = $state<EngineId>("postgres");
  let colorId = $state<ConnectionColorId>("teal");
  let host = $state("127.0.0.1");
  let port = $state(5432);
  let database = $state("");
  let path = $state("");
  let uri = $state("");
  let username = $state("");
  let password = $state("");
  let ssl = $state(false);
  let readOnly = $state(true);
  let environment = $state<Environment>("local");
  let notes = $state("");
  let formError = $state<string | null>(null);

  const isEdit = $derived(!!initial?.id);
  const engineLocked = $derived(isEdit || !!lockedEngine);

  const selectedEngine = $derived(ENGINE_OPTIONS.find((e) => e.id === engine));
  const colorHexVal = $derived(
    CONNECTION_COLORS.find((c) => c.id === colorId)?.hex ?? "#4ecdc4",
  );

  const engineHint = $derived(
    selectedEngine
      ? `${selectedEngine.description}`
      : undefined,
  );

  function applyDraft(d: ConnectionFormDraft) {
    name = d.name;
    engine = d.engine;
    colorId = d.colorId;
    host = d.host;
    port = d.port;
    database = d.database;
    path = d.path;
    uri = d.uri;
    username = d.username;
    password = d.password;
    ssl = d.ssl;
    readOnly = d.readOnly;
    environment = d.environment;
    notes = d.notes;
  }

  // Prefill when opening edit / create with locked engine.
  $effect(() => {
    formError = null;
    if (initial) {
      applyDraft(draftFromProfile(initial));
    } else {
      const eng = lockedEngine ?? "postgres";
      applyDraft(emptyDraft(eng));
    }
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
      if (!path.trim()) {
        formError = "SQLite database file path is required.";
        return;
      }
    } else if (engine === "mongodb") {
      if (!uri.trim() && (!host.trim() || !database.trim())) {
        formError = "Provide a connection URI, or host and database name.";
        return;
      }
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
        port: Number(port) || selectedEngine?.defaultPort || 5432,
        database: database.trim(),
        path: path.trim(),
        uri: uri.trim(),
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
    title={isEdit ? "Edit connection" : "Connection details"}
    description={isEdit
      ? "Update connection details. Leave password blank to keep the existing secret."
      : "Fill in how Mystique should reach this database. Saving profiles is local for now — live connect comes with engine plugins."}
  >
    {#snippet actions()}
      {#if !isEdit && onback}
        <Button variant="ghost" onclick={onback} disabled={saving}>
          Change engine
        </Button>
      {/if}
    {/snippet}
  </PageHeader>

  <form class="m-0" onsubmit={submit}>
    <Stack gap="md">
      <FormGrid>
        <Field label="Database engine" hint={engineHint}>
          {#if engineLocked}
            <div
              class="flex h-control items-center gap-2 rounded-md border border-line bg-input px-2.5"
            >
              <EngineBadge {engine} size="md" />
              <span class="text-body font-medium text-fg">
                {selectedEngine?.name ?? engine}
              </span>
              <Badge tone="neutral" size="sm">
                {selectedEngine?.queryLanguage ?? "—"}
              </Badge>
            </div>
          {:else}
            <Select bind:value={engine}>
              {#each ENGINE_OPTIONS as eng (eng.id)}
                <option value={eng.id} disabled={!eng.available}>
                  {eng.name}{eng.available ? "" : " (coming soon)"}
                </option>
              {/each}
            </Select>
          {/if}
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
          placeholder={engine === "mongodb"
            ? "e.g. local-mongo"
            : engine === "sqlite"
              ? "e.g. app-dev.sqlite"
              : "e.g. local-pg"}
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

      {#if engine === "sqlite"}
        <Card title="Database file">
          <Field
            label="File path"
            hint="Absolute or relative path to the .db / .sqlite file on this machine."
            full
          >
            <Input
              bind:value={path}
              placeholder="/path/to/database.sqlite"
              autocomplete="off"
              class="font-mono"
            />
          </Field>
          <div class="mt-3">
            <Checkbox bind:checked={readOnly} label="Read-only (recommended)" />
          </div>
        </Card>
      {:else if engine === "mongodb"}
        <Card title="Server">
          <Field
            label="Connection URI (optional)"
            hint="If set, URI is preferred over host fields when connecting."
            full
          >
            <Input
              bind:value={uri}
              placeholder="mongodb://user:pass@127.0.0.1:27017/mydb"
              autocomplete="off"
              class="font-mono"
            />
          </Field>
          <div class="mt-3">
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
                  ? "Leave blank to keep the existing secret."
                  : "Stored only when keychain is wired; for now UI-only."}
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
          </div>
          <div class="mt-3 flex flex-wrap gap-4">
            <Checkbox bind:checked={ssl} label="Require TLS" />
            <Checkbox bind:checked={readOnly} label="Read-only (recommended)" />
          </div>
        </Card>
      {:else}
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
                ? "Leave blank to keep the existing secret."
                : "Keychain storage comes next; profile is saved without a live session for now."}
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
      {/if}

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
