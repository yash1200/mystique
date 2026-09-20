<script lang="ts">
	import { connectionKindFromCaps } from '../capabilities'
	import { adapterFor } from '../registry'
	import type { EngineFieldsProps } from '../types'
	import FileFields from './FileFields.svelte'
	import HostFields from './HostFields.svelte'
	import SshFields from './SshFields.svelte'
	import SslFields from './SslFields.svelte'
	import UriFields from './UriFields.svelte'

	let {
		draft = $bindable(),
		isEdit = false,
		passwordSet = false,
		sshPasswordSet = false,
		disabled = false,
	}: EngineFieldsProps = $props()

	const kind = $derived(
		connectionKindFromCaps(
			adapterFor(draft.engine).defaultCapabilities,
		),
	)
</script>

{#if kind === 'file'}
	<FileFields bind:draft {isEdit} {passwordSet} {disabled} />
{:else if kind === 'uri'}
	<UriFields bind:draft {isEdit} {passwordSet} {disabled} />
	<SslFields bind:draft {isEdit} {passwordSet} {disabled} />
	<SshFields bind:draft {isEdit} {sshPasswordSet} {disabled} />
{:else}
	<HostFields bind:draft {isEdit} {passwordSet} {disabled} />
	<SslFields bind:draft {isEdit} {passwordSet} {disabled} />
	<SshFields bind:draft {isEdit} {sshPasswordSet} {disabled} />
{/if}
