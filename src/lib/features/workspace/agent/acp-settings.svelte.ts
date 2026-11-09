/**
 * External (ACP) agents: the added list, the default for new chats, and
 * what detection found on this machine (Settings → AI → Agent). Shared by
 * the settings modal and the agent pane's agent menu.
 */

import { errorMessage } from '#core/ipc'
import {
	addDetectedAcpAgent,
	detectAcpAgents,
	getAcpSettings,
	removeAcpAgent,
	saveAcpAgent,
	setDefaultAgent,
	testAcpAgent,
} from './api'
import type {
	AcpAgentSaved,
	AcpAgentView,
	AcpProbe,
	AcpSettingsView,
	DetectedAcpAgent,
	SaveAcpAgent,
} from './types'

/** The provider tag of an external agent's turns and chats. */
export const ACP_PROVIDER = 'acp'

/** The built-in agent's id in the agent menu. */
export const BUILTIN_AGENT = 'builtin'

/** Mirrors the backend: names that usually hold a credential go to the vault. */
export function looksSecret(name: string): boolean {
	const upper = name.toUpperCase()
	return ['KEY', 'TOKEN', 'SECRET', 'PASSWORD', 'PASSWD', 'CREDENTIAL', 'AUTH'].some(
		(word) => upper.includes(word),
	)
}

/**
 * Split a command line typed in one field into words. Double or single
 * quotes keep spaces; nothing is expanded (the agent runs without a shell).
 */
export function splitArgs(line: string): string[] {
	const out: string[] = []
	let word = ''
	let quote: '"' | "'" | null = null
	let started = false
	for (const ch of line) {
		if (quote) {
			if (ch === quote) quote = null
			else word += ch
		} else if (ch === '"' || ch === "'") {
			quote = ch
			started = true
		} else if (/\s/.test(ch)) {
			if (started || word) out.push(word)
			word = ''
			started = false
		} else {
			word += ch
			started = true
		}
	}
	if (started || word) out.push(word)
	return out
}

/** Arguments back into one editable line. */
export function joinArgs(args: string[]): string {
	return args.map((a) => (a === '' || /[\s"']/.test(a) ? `"${a}"` : a)).join(' ')
}

/** The added agent `id`, if it still exists. */
export function findAgent(
	view: AcpSettingsView | null | undefined,
	id: string | null | undefined,
): AcpAgentView | null {
	if (!view || !id) return null
	return view.agents.find((a) => a.id === id) ?? null
}

/** Built-in first, then every added agent: the pane's agent menu. */
export function agentChoices(
	view: AcpSettingsView | null | undefined,
): { value: string; label: string }[] {
	return [
		{ value: BUILTIN_AGENT, label: 'Built-in' },
		...(view?.agents ?? []).map((a) => ({ value: a.id, label: a.label })),
	]
}

/** What a new chat uses: the default, or built-in if it was removed. */
export function defaultChoice(view: AcpSettingsView | null | undefined): string {
	const id = view?.defaultAgent ?? BUILTIN_AGENT
	return findAgent(view, id) ? id : BUILTIN_AGENT
}

/**
 * The agent a saved chat talks to. An external agent's chat keeps the
 * agent's id as its `model`; a chat whose agent was removed falls back to
 * the default. Any other chat is the built-in agent's.
 */
export function chatChoice(
	view: AcpSettingsView | null | undefined,
	chat: { provider: string; model: string } | null | undefined,
): string {
	if (!chat) return defaultChoice(view)
	if (chat.provider !== ACP_PROVIDER) return BUILTIN_AGENT
	return findAgent(view, chat.model) ? chat.model : defaultChoice(view)
}

/** Detected agents in two groups: on this machine, and not installed. */
export function groupDetected(list: DetectedAcpAgent[]): {
	found: DetectedAcpAgent[]
	missing: DetectedAcpAgent[]
} {
	return {
		found: list.filter((a) => a.status !== 'missing'),
		missing: list.filter((a) => a.status === 'missing'),
	}
}

/** One short line on where a detected agent stands. */
export function statusLabel(agent: DetectedAcpAgent): string {
	switch (agent.status) {
		case 'installed':
			return 'Installed'
		case 'adapter': {
			const runner = agent.command?.split(/[\\/]/).pop() ?? 'npx'
			return `Adapter via ${runner.replace(/\.(cmd|exe|bat)$/i, '')}`
		}
		case 'unconfirmed':
			return 'ACP support not confirmed'
		default:
			return 'Not installed'
	}
}

/** True when Add can set it up with one click. */
export function canAdd(agent: DetectedAcpAgent): boolean {
	return (
		agent.confirmed &&
		(agent.status === 'installed' || agent.status === 'adapter') &&
		Boolean(agent.command)
	)
}

/** The Test result in a line: transport and sign-in. */
export function probeLine(probe: AcpProbe): string {
	const parts = [`ACP v${probe.protocolVersion}`]
	if (probe.mcpTransport === 'http') parts.push('MCP over HTTP')
	else if (probe.mcpTransport === 'stdio') parts.push('MCP through the stdio bridge')
	else parts.push('cannot reach MCP')
	if (probe.signedIn === true) parts.push('signed in')
	else if (probe.signedIn === false) parts.push('not signed in')
	return parts.join(' · ')
}

class AcpSettingsStore {
	view = $state<AcpSettingsView | null>(null)
	error = $state<string | null>(null)
	detected = $state<DetectedAcpAgent[] | null>(null)
	detecting = $state(false)
	#inflight: Promise<AcpSettingsView> | null = null
	#detecting: Promise<DetectedAcpAgent[]> | null = null

	load = async (force = false): Promise<AcpSettingsView | null> => {
		if (!force && this.view) return this.view
		if (!force && this.#inflight) return this.#inflight
		this.error = null
		const task = getAcpSettings()
			.then((view) => {
				this.view = view
				return view
			})
			.catch((e) => {
				this.error = errorMessage(e)
				throw e
			})
			.finally(() => {
				this.#inflight = null
			})
		this.#inflight = task
		return task
	}

	/** Look for agents on this machine (Rescan). */
	detect = async (): Promise<DetectedAcpAgent[]> => {
		if (this.#detecting) return this.#detecting
		this.detecting = true
		const task = detectAcpAgents()
			.then((list) => {
				this.detected = list
				return list
			})
			.finally(() => {
				this.detecting = false
				this.#detecting = null
			})
		this.#detecting = task
		return task
	}

	#marked(id: string, catalogId: string | null | undefined) {
		if (!catalogId || !this.detected) return
		this.detected = this.detected.map((a) =>
			a.catalogId === catalogId ? { ...a, addedId: id } : a,
		)
	}

	/** Add a detected agent with one click. */
	add = async (catalogId: string): Promise<AcpAgentSaved> => {
		const saved = await addDetectedAcpAgent(catalogId)
		this.view = saved.settings
		this.#marked(saved.id, catalogId)
		return saved
	}

	/** Add or change an agent from the Custom agent form. */
	save = async (input: SaveAcpAgent): Promise<AcpAgentSaved> => {
		const saved = await saveAcpAgent(input)
		this.view = saved.settings
		this.#marked(saved.id, input.catalogId)
		return saved
	}

	remove = async (id: string): Promise<void> => {
		this.view = await removeAcpAgent(id)
		if (this.detected) {
			this.detected = this.detected.map((a) =>
				a.addedId === id ? { ...a, addedId: null } : a,
			)
		}
	}

	setDefault = async (id: string): Promise<void> => {
		this.view = await setDefaultAgent(id)
	}

	/** Start agent `id`, run `initialize` and a sign-in check; refreshes its label. */
	test = async (id: string): Promise<AcpProbe> => {
		const probe = await testAcpAgent(id)
		try {
			this.view = await getAcpSettings()
		} catch {
			/* the label refreshes on the next load */
		}
		return probe
	}
}

export const acpSettings = new AcpSettingsStore()
