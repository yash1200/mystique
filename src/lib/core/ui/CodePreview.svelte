<script lang="ts">
	import { cn, highlightCode, tokenClass, type SyntaxLang } from '#core/util'

	/** "Will run": the statement a dialog is about to send, highlighted. */
	interface Props {
		code: string
		lang?: SyntaxLang
		class?: string
	}

	let { code, lang = 'sql', class: className = '' }: Props = $props()

	const tokens = $derived(highlightCode(lang, code))
</script>

<pre
	class={cn(
		'm-0 max-h-64 overflow-auto rounded-[9px] bg-field px-3 py-2.5 font-mono text-label leading-relaxed break-all whitespace-pre-wrap text-fg-secondary shadow-[inset_0_0_0_1px_var(--line)] select-text',
		className,
	)}>{#each tokens as token, i (i)}<span class={tokenClass(token.kind)}>{token.text}</span>{/each}</pre>
