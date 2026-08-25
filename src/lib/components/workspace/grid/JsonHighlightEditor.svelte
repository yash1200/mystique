<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import { highlightCode, tokenClass } from '$lib/utils/highlightCode'

	interface Props {
		value?: string
		readonly?: boolean
		invalid?: boolean
		class?: string
	}

	let {
		value = $bindable(''),
		readonly = false,
		invalid = false,
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

	const surface =
		'whitespace-pre-wrap break-words px-2.5 py-2 font-mono text-label leading-relaxed'
</script>

<div
	class={cn(
		'relative min-h-64 flex-1 overflow-hidden rounded-md border border-line bg-input',
		'focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted',
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
		aria-hidden="true"
	>{#each tokens as tok, i (i)}<span class={tokenClass(tok.kind)}>{tok.text}</span>{/each}</pre>
	<textarea
		bind:value
		{readonly}
		spellcheck={false}
		class={cn(
			'absolute inset-0 z-10 resize-none overflow-auto border-0 bg-transparent text-transparent caret-fg outline-none selection:bg-selected',
			surface,
		)}
		onscroll={syncScroll}
	></textarea>
</div>
