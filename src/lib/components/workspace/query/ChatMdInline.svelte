<script lang="ts">
	import type { MdInline } from '$lib/utils/chatMarkdown'

	interface Props {
		nodes: MdInline[]
	}

	let { nodes }: Props = $props()
</script>

{#each nodes as node, i (i)}{#if node.type === 'text'}{node.value}{:else if node.type === 'break'}<br
		/>{:else if node.type === 'code'}<code
			class="rounded-sm bg-input px-1 py-px font-mono text-caption text-syntax-name"
			>{node.value}</code
		>{:else if node.type === 'strong'}<strong class="font-semibold text-fg"
			><ChatMdInline nodes={node.children} /></strong
		>{:else if node.type === 'em'}<em class="italic"
			><ChatMdInline nodes={node.children} /></em
		>{:else if node.type === 'del'}<s class="text-fg-secondary"
			><ChatMdInline nodes={node.children} /></s
		>{:else if node.type === 'link' && node.href}<a
			href={node.href}
			target="_blank"
			rel="external noreferrer"
			class="text-accent underline-offset-2 hover:underline"
			><ChatMdInline nodes={node.children} /></a
		>
		>{:else if node.type === 'link'}<ChatMdInline
			nodes={node.children}
		/>{/if}{/each}
