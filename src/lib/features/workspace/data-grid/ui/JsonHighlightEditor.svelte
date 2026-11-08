<script lang="ts">
	import { cn, highlightCode, tokenClass } from '#core/util'
	interface Props {
		value?: string
		readonly?: boolean
		invalid?: boolean
		/** Fill a parent instead of the modal’s min-height card. */
		fill?: boolean
		class?: string
	}

	let {
		value = $bindable(''),
		readonly = false,
		invalid = false,
		fill = false,
		class: className = '',
	}: Props = $props()

	let preEl = $state<HTMLPreElement | null>(null)

	const tokens = $derived.by(() => {
		try {
			return highlightCode('json', value)
		} catch {
			return [{ kind: 'text' as const, text: value }]
		}
	})

	function syncScroll(e: Event) {
		const el = e.currentTarget as HTMLTextAreaElement
		if (!preEl) return
		preEl.scrollTop = el.scrollTop
		preEl.scrollLeft = el.scrollLeft
	}

	const surface = $derived(
		fill
			? 'whitespace-pre-wrap break-words px-3 py-2.5 font-mono text-label leading-relaxed'
			: 'whitespace-pre-wrap break-words px-2.5 py-2 font-mono text-label leading-relaxed',
	)
</script>

<div
	class={cn(
		'relative overflow-hidden bg-input',
		fill
			? 'min-h-0 flex-1'
			: 'min-h-64 flex-1 rounded-md border border-line focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted',
		invalid && 'border-danger/50',
		className,
	)}
>
	<pre
		bind:this={preEl}
		class={cn(
			'pointer-events-none absolute inset-0 m-0 overflow-auto text-fg',
			surface,
		)}
		aria-hidden="true">{#each tokens as tok, i (i)}<span
				class={tokenClass(tok.kind)}>{tok.text}</span
			>{/each}</pre>
	<textarea
		bind:value
		{readonly}
		spellcheck={false}
		aria-label="JSON"
		class={cn(
			'absolute inset-0 z-10 resize-none overflow-auto border-0 bg-transparent text-transparent caret-fg outline-none selection:bg-selected',
			surface,
		)}
		onscroll={syncScroll}
	></textarea>
</div>
