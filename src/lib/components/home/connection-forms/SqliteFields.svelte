<script lang="ts">
  import { Card, Checkbox, Field, Input } from "$lib/components/ui";

  interface Props {
    path?: string;
    readOnly?: boolean;
  }

  let {
    path = $bindable(""),
    readOnly = $bindable(true),
  }: Props = $props();
</script>

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
