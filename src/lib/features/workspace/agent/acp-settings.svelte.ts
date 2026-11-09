/**
 * Which agent the pane runs: built-in (default) or an external ACP agent
 * (Settings → AI → Agent). Shared by the settings modal and the pane.
 */

import { errorMessage } from '#core/ipc'
import { getAcpSettings, saveAcpSettings, testAcpAgent } from './api'
import type { AcpProbe, AcpSettingsView, SaveAcpSettings } from './types'

/** The provider tag of an external agent's turns and chats. */
export const ACP_PROVIDER = 'acp'

/** True when the pane runs the external agent. */
export function isExternalAgent(view: AcpSettingsView | null | undefined): boolean {
	return view?.kind === 'acp' && view.command.trim() !== ''
}

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

class AcpSettingsStore {
	view = $state<AcpSettingsView | null>(null)
	error = $state<string | null>(null)
	#inflight: Promise<AcpSettingsView> | null = null

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

	save = async (input: SaveAcpSettings) => {
		this.view = await saveAcpSettings(input)
		return this.view
	}

	/** Runs `initialize` against the saved command; refreshes the label. */
	test = async (): Promise<AcpProbe> => {
		const probe = await testAcpAgent()
		try {
			this.view = await getAcpSettings()
		} catch {
			/* the label refreshes on the next load */
		}
		return probe
	}
}

export const acpSettings = new AcpSettingsStore()
