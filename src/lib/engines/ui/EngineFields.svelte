<script lang="ts">
	import { connectionKindFromCaps } from '../capabilities'
	import { adapterFor } from '../registry'
	import type { EngineFieldsProps } from '../types'
	import FileFields from './FileFields.svelte'
	import HostFields from './HostFields.svelte'
	import SshFields from './SshFields.svelte'
	import SslFields from './SslFields.svelte'
	import UriFields from './UriFields.svelte'

	/**
	 * The engine's own fields, in two parts so the form can put each in its
	 * own card: `server` (where it is, who signs in) and `security` (TLS and
	 * the SSH tunnel). A file engine has no security part.
	 */
	let {
		draft = $bindable(),
		section,
		isEdit = false,
		passwordSet = false,
		sshPasswordSet = false,
		disabled = false,
	}: EngineFieldsProps & { section: 'server' | 'security' } = $props()

	const kind = $derived(
		connectionKindFromCaps(adapterFor(draft.engine).defaultCapabilities),
	)
</script>

{#if section === 'server'}
	{#if kind === 'file'}
		<FileFields bind:draft {isEdit} {passwordSet} {disabled} />
	{:else if kind === 'uri'}
		<UriFields bind:draft {isEdit} {passwordSet} {disabled} />
	{:else}
		<HostFields bind:draft {isEdit} {passwordSet} {disabled} />
	{/if}
{:else if kind !== 'file'}
	<SslFields bind:draft {isEdit} {passwordSet} {disabled} />
	<div class="h-px bg-line" role="separator"></div>
	<SshFields bind:draft {isEdit} {sshPasswordSet} {disabled} />
{/if}
