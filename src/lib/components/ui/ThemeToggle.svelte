<script lang="ts">
  import { cn } from "$lib/utils/cn";

  type ThemePref = "light" | "dark" | "system";

  interface Props {
    preference?: ThemePref;
    onchange?: (pref: ThemePref) => void;
  }

  let { preference = "dark", onchange }: Props = $props();

  const options: { id: ThemePref; label: string; icon: string }[] = [
    { id: "light", label: "Light", icon: "☀" },
    { id: "dark", label: "Dark", icon: "☾" },
    { id: "system", label: "System", icon: "◌" },
  ];
</script>

<div
  class="inline-flex gap-0.5 rounded-md border border-line bg-input p-0.5"
  role="group"
  aria-label="Theme"
>
  {#each options as opt (opt.id)}
    <button
      type="button"
      class={cn(
        "inline-flex items-center gap-1 rounded-sm border-none px-2 py-1 text-caption font-medium transition-colors",
        preference === opt.id
          ? "bg-elevated text-fg shadow-sm"
          : "bg-transparent text-fg-muted hover:bg-hover hover:text-fg-secondary",
      )}
      title={opt.label}
      aria-pressed={preference === opt.id}
      onclick={() => onchange?.(opt.id)}
    >
      <span aria-hidden="true">{opt.icon}</span>
      <span class="hidden min-[1100px]:inline">{opt.label}</span>
    </button>
  {/each}
</div>
