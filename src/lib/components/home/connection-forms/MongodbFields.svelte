<script lang="ts">
  import { Card, Checkbox, Field, Input } from "$lib/components/ui";

  interface Props {
    uri?: string;
    readOnly?: boolean;
  }

  let {
    uri = $bindable(""),
    readOnly = $bindable(true),
  }: Props = $props();
</script>

<Card title="Connection">
  <Field
    label="Connection URI"
    hint="Include credentials in the URI when needed (e.g. mongodb://user:pass@host:27017/db)."
    full
  >
    <Input
      bind:value={uri}
      placeholder="mongodb://127.0.0.1:27017/mydb"
      autocomplete="off"
      class="font-mono"
    />
  </Field>
  <div class="mt-3">
    <Checkbox bind:checked={readOnly} label="Read-only (recommended)" />
  </div>
</Card>
