<script lang="ts">
  import {
    CONNECTION_COLORS,
    ENGINE_OPTIONS,
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
    oncancel?: () => void;
    onsave?: (draft: {
      name: string;
      engine: EngineId;
      colorId: ConnectionColorId;
      host: string;
      port: number;
      database: string;
      username: string;
      password: string;
      ssl: boolean;
      readOnly: boolean;
      environment: Environment;
      notes: string;
    }) => void;
    ontest?: () => void;
  }

  let { oncancel, onsave, ontest }: Props = $props();

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

  const selectedEngine = $derived(ENGINE_OPTIONS.find((e) => e.id === engine));
  const colorHexVal = $derived(
    CONNECTION_COLORS.find((c) => c.id === colorId)?.hex ?? "#4ecdc4",
  );

  const engineHint = $derived(
    selectedEngine
      ? `${selectedEngine.description} · ${selectedEngine.queryLanguage}`
      : undefined,
  );

  $effect(() => {
    if (engine === "postgres") port = 5432;
    else if (engine === "mysql") port = 3306;
    else if (engine === "mongodb") port = 27017;
    else if (engine === "sqlite") port = 0;
  });

  function submit(e: Event) {
    e.preventDefault();
    formError = null;
    if (!name.trim()) {
      formError = "Give this connection a name.";
      return;
    }
    if (!selectedEngine?.available) {
      formError = "Selected engine is not available yet.";
      return;
    }
    if (!host.trim() || !database.trim()) {
      formError = "Host and database are required.";
      return;
    }
    onsave?.({
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
  }
</script>

<ContentPanel size="md" ariaLabel="Create connection">
  <PageHeader
    title="New connection"
    description="Choose an engine, then fill connection details. Secrets stay local — backend wiring comes later."
  />

  <form class="m-0" onsubmit={submit}>
    <Stack gap="md">
      <FormGrid>
        <Field label="Database engine" hint={engineHint}>
          <Select bind:value={engine}>
            {#each ENGINE_OPTIONS as eng (eng.id)}
              <option value={eng.id} disabled={!eng.available}>
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
          <Field label="Password" full>
            <Input
              type="password"
              bind:value={password}
              autocomplete="new-password"
              placeholder="Stored securely later"
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
          <Button variant="ghost" onclick={oncancel}>Cancel</Button>
        {/if}
        <Button variant="ghost" onclick={ontest}>Test connection</Button>
        <Button type="submit" variant="primary">Save connection</Button>
      </Inline>
    </Stack>
  </form>
</ContentPanel>
