<script lang="ts">
	import { Button, Icon, IconButton } from '#core/ui'
	import {
		cn,
		copyTextToClipboard,
		highlightCode,
		tokenClass,
	} from '#core/util'
	import { onDestroy } from 'svelte'
	import { inferFenceLang, isQueryLang } from '../markdown'
	import { classifyLang } from '../prettify'

	interface Props {
		code: string
		lang?: string
		engine: string
		compact?: boolean
		onApply?: (sql: string) => void
	}

	let {
		code,
		lang = '',
		engine: _engine,
		compact = false,
		onApply,
	}: Props = $props()

	const resolvedLang = $derived(inferFenceLang(lang, code))
	const display = $derived(code.endsWith('\n') ? code : `${code}\n`)
	const tokens = $derived.by(() => {
		try {
			return highlightCode(classifyLang(resolvedLang, display), display)
		} catch {
			return [{ kind: 'text' as const, text: display }]
		}
	})
	const applyable = $derived(isQueryLang(resolvedLang) && Boolean(onApply))
	const langLabel = $derived(labelFor(resolvedLang))

	let copied = $state(false)
	let copyTimer = $state<ReturnType<typeof setTimeout> | null>(null)

	function labelFor(id: string): string {
		switch (id) {
			case 'postgresql':
			case 'postgres':
				return 'PostgreSQL'
			case 'mysql':
			case 'mariadb':
				return 'MySQL'
			case 'sqlite':
				return 'SQLite'
			case 'sql':
			case 'plsql':
			case 'tsql':
				return 'SQL'
			case 'mql':
			case 'mongodb':
			case 'mongo':
				return 'MQL'
			case 'javascript':
			case 'js':
				return 'JavaScript'
			case 'json':
				return 'JSON'
			case 'dbml':
				return 'DBML'
			default:
				return id ? id.toUpperCase() : 'Code'
		}
	}

	async function copy() {
		const result = await copyTextToClipboard(display.replace(/\s+$/, '') + '\n')
		if (!result.ok) return
		copied = true
		if (copyTimer) clearTimeout(copyTimer)
		copyTimer = setTimeout(() => {
			copied = false
		}, 1400)
	}

	onDestroy(() => {
		if (copyTimer) clearTimeout(copyTimer)
	})
</script>

<figure
	class={cn(
		'm-0 max-w-full min-w-0 overflow-hidden rounded-lg border border-line bg-app',
		compact && 'rounded-md',
	)}
>
	<figcaption
		class="flex items-center gap-1 border-b border-line bg-elevated px-2 py-1"
	>
		<Icon name="code" class="text-fg-muted" />
		<span
			class="min-w-0 flex-1 truncate font-mono text-micro font-semibold tracking-wider text-fg-muted uppercase"
		>
			{langLabel}
		</span>
		<IconButton
			label={copied ? 'Copied' : 'Copy code'}
			variant="ghost"
			size="sm"
			onclick={() => void copy()}
		>
			<Icon name={copied ? 'check' : 'copy'} />
		</IconButton>
		{#if applyable && onApply}
			<Button size="sm" variant="ghost" onclick={() => onApply(display)}>
				Apply
			</Button>
		{/if}
	</figcaption>
	<pre
		class={cn(
			'm-0 overflow-x-auto p-2.5 font-mono text-caption leading-normal',
			compact && 'p-2',
		)}><code class="block whitespace-pre text-fg"
			>{#each tokens as tok, i (i)}<span class={tokenClass(tok.kind)}
					>{tok.text}</span
				>{/each}</code
		></pre>
</figure>
