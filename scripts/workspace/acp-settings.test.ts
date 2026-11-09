import { flushSync, mount, tick, unmount } from 'svelte'
import { afterEach, describe, expect, it } from 'vitest'
import type {
	AcpAgentView,
	AcpProbe,
	AcpSettingsView,
	DetectedAcpAgent,
} from '../../src/lib/core/contracts'
import AgentBackendSettings from '../../src/lib/features/settings/ui/AgentBackendSettings.svelte'
import {
	acpSettings,
	agentChoices,
	BUILTIN_AGENT,
	canAdd,
	chatChoice,
	defaultChoice,
	groupDetected,
	joinArgs,
	looksSecret,
	probeLine,
	splitArgs,
	statusLabel,
} from '../../src/lib/features/workspace/agent/acp-settings.svelte'
import { mockIpc } from './mock-ipc'

function agent(patch: Partial<AcpAgentView>): AcpAgentView {
	return {
		id: 'claude',
		catalogId: 'claude',
		label: 'Claude Code',
		command: '/usr/local/bin/npx',
		args: ['-y', '@agentclientprotocol/claude-agent-acp'],
		env: [],
		...patch,
	}
}

function view(patch: Partial<AcpSettingsView> = {}): AcpSettingsView {
	return { defaultAgent: BUILTIN_AGENT, agents: [], ...patch }
}

function detected(patch: Partial<DetectedAcpAgent>): DetectedAcpAgent {
	return {
		catalogId: 'agy',
		name: 'Antigravity CLI',
		mark: 'Ag',
		status: 'installed',
		path: '/Users/me/.local/bin/agy',
		version: '1.5.3',
		command: '/Users/me/.local/bin/agy',
		args: ['acp'],
		mcp: 'stdio',
		confirmed: true,
		signIn: 'Run `agy` once.',
		install: 'curl -fsSL https://antigravity.google/cli/install.sh | bash',
		homepage: 'https://antigravity.google',
		source: 'https://antigravity.google/docs/cli/acp/',
		note: null,
		addedId: null,
		...patch,
	}
}

function probe(patch: Partial<AcpProbe> = {}): AcpProbe {
	return {
		agentId: 'agy',
		name: 'Antigravity ACP Server',
		title: null,
		version: '1.5.3',
		protocolVersion: 1,
		mcpHttp: false,
		mcpSse: false,
		mcpTransport: 'stdio',
		loadSession: true,
		authMethods: [],
		signedIn: true,
		problem: null,
		...patch,
	}
}

function resetStore() {
	acpSettings.view = null
	acpSettings.detected = null
	acpSettings.detecting = false
}

describe('external agent helpers', () => {
	it('splits arguments without a shell and joins them back', () => {
		expect(splitArgs('-y  @agentclientprotocol/claude-agent-acp')).toEqual([
			'-y',
			'@agentclientprotocol/claude-agent-acp',
		])
		expect(splitArgs(`--cwd "/My Folder" '' x`)).toEqual(['--cwd', '/My Folder', '', 'x'])
		expect(splitArgs('   ')).toEqual([])
		const args = ['--cwd', '/My Folder', '', '$HOME']
		expect(splitArgs(joinArgs(args))).toEqual(args)
	})

	it('flags names that usually hold a credential, like the backend', () => {
		expect(looksSecret('ANTHROPIC_API_KEY')).toBe(true)
		expect(looksSecret('github_token')).toBe(true)
		expect(looksSecret('CLAUDE_CODE_EXECUTABLE')).toBe(false)
	})

	it('lists built-in first, then the added agents', () => {
		expect(agentChoices(null)).toEqual([{ value: 'builtin', label: 'Built-in' }])
		expect(agentChoices(view({ agents: [agent({}), agent({ id: 'custom-1', label: 'Mine' })] }))).toEqual([
			{ value: 'builtin', label: 'Built-in' },
			{ value: 'claude', label: 'Claude Code' },
			{ value: 'custom-1', label: 'Mine' },
		])
	})

	it('new chats use the default; saved chats remember their agent', () => {
		const v = view({ defaultAgent: 'claude', agents: [agent({}), agent({ id: 'agy' })] })
		expect(defaultChoice(v)).toBe('claude')
		expect(defaultChoice(view({ defaultAgent: 'gone' }))).toBe(BUILTIN_AGENT)
		expect(chatChoice(v, null)).toBe('claude')
		expect(chatChoice(v, { provider: 'acp', model: 'agy' })).toBe('agy')
		// The chat's agent was removed: the default takes over.
		expect(chatChoice(v, { provider: 'acp', model: 'removed' })).toBe('claude')
		// An older external chat (model was the agent's name) also falls back.
		expect(chatChoice(v, { provider: 'acp', model: 'Claude Code' })).toBe('claude')
		expect(chatChoice(v, { provider: 'anthropic', model: 'claude-x' })).toBe(BUILTIN_AGENT)
	})

	it('groups detected agents and says where each stands', () => {
		const list = [
			detected({}),
			detected({
				catalogId: 'claude',
				status: 'adapter',
				command: '/opt/homebrew/bin/npx',
			}),
			detected({ catalogId: 'maybe', status: 'unconfirmed', confirmed: false, command: null }),
			detected({ catalogId: 'goose', status: 'missing', command: null, path: null }),
		]
		const { found, missing } = groupDetected(list)
		expect(found.map((a) => a.catalogId)).toEqual(['agy', 'claude', 'maybe'])
		expect(missing.map((a) => a.catalogId)).toEqual(['goose'])
		expect(found.map(statusLabel)).toEqual([
			'Installed',
			'Adapter via npx',
			'ACP support not confirmed',
		])
		expect(statusLabel(missing[0])).toBe('Not installed')
		expect(found.map(canAdd)).toEqual([true, true, false])
		expect(canAdd(missing[0])).toBe(false)
	})

	it('sums up a test in one line', () => {
		expect(probeLine(probe())).toBe('ACP v1 · MCP through the stdio bridge · signed in')
		expect(probeLine(probe({ mcpTransport: 'http', signedIn: false }))).toBe(
			'ACP v1 · MCP over HTTP · not signed in',
		)
		expect(probeLine(probe({ mcpTransport: null, signedIn: null }))).toBe(
			'ACP v1 · cannot reach MCP',
		)
	})
})

describe('agent list store', () => {
	afterEach(resetStore)

	it('marks a detected agent as added, and clears it on remove', async () => {
		resetStore()
		const added = agent({ id: 'agy', catalogId: 'agy', label: 'Antigravity CLI' })
		const calls = mockIpc({
			detect_acp_agents: () => [detected({})],
			add_detected_acp_agent: () => ({
				id: 'agy',
				settings: view({ defaultAgent: 'agy', agents: [added] }),
			}),
			remove_acp_agent: () => view(),
		})
		await acpSettings.detect()
		const saved = await acpSettings.add('agy')
		expect(saved.id).toBe('agy')
		expect(acpSettings.view?.defaultAgent).toBe('agy')
		expect(acpSettings.detected?.[0].addedId).toBe('agy')
		await acpSettings.remove('agy')
		expect(acpSettings.detected?.[0].addedId).toBeNull()
		expect(calls.map((c) => c.command)).toEqual([
			'detect_acp_agents',
			'add_detected_acp_agent',
			'remove_acp_agent',
		])
		expect(calls[1].args).toEqual({ catalogId: 'agy' })
		expect(calls[2].args).toEqual({ id: 'agy' })
	})
})

async function settle() {
	for (let i = 0; i < 6; i++) {
		await tick()
		await new Promise((r) => setTimeout(r, 0))
	}
	flushSync()
}

function button(root: HTMLElement, label: string): HTMLButtonElement {
	const found = [...root.querySelectorAll('button')].find(
		(b) => b.textContent?.trim() === label,
	)
	if (!found) throw new Error(`no button "${label}" in ${root.textContent}`)
	return found
}

describe('Settings → AI → Agent', () => {
	afterEach(resetStore)

	it('shows built-in, detected agents with Add, and the not-installed ones collapsed', async () => {
		resetStore()
		let settings = view()
		const calls = mockIpc({
			get_acp_settings: () => settings,
			detect_acp_agents: () => [
				detected({}),
				detected({
					catalogId: 'maybe',
					name: 'Maybe Agent',
					status: 'unconfirmed',
					confirmed: false,
					command: null,
				}),
				detected({ catalogId: 'goose', name: 'goose', status: 'missing', command: null, path: null }),
			],
			add_detected_acp_agent: () => {
				settings = view({
					defaultAgent: 'agy',
					agents: [agent({ id: 'agy', catalogId: 'agy', label: 'Antigravity CLI' })],
				})
				return { id: 'agy', settings }
			},
			test_acp_agent: () =>
				probe({ signedIn: false, problem: 'Not signed in. Run `agy` once.' }),
		})
		const target = document.createElement('div')
		document.body.appendChild(target)
		const component = mount(AgentBackendSettings, { target })
		try {
			await settle()
			const text = target.textContent ?? ''
			expect(text).toContain('Built-in')
			expect(text).toMatch(/Detected on this (Mac|computer)/)
			expect(text).toContain('Antigravity CLI')
			expect(text).toContain('1.5.3')
			expect(text).toContain('/Users/me/.local/bin/agy')
			// Unconfirmed: shown, said plainly, no Add.
			const maybe = target.querySelector('[data-status="unconfirmed"]')
			expect(maybe?.textContent).toContain('ACP support not confirmed')
			expect(maybe?.querySelector('button')).toBeNull()
			// Missing ones are collapsed behind a count.
			expect(text).toContain('Not installed (1)')
			expect(text).not.toContain('Copy command')
			button(target, 'Not installed (1)').click()
			await settle()
			expect(target.textContent).toContain('Copy command')
			expect(target.textContent).toContain('antigravity.google/cli/install.sh')

			// One click: added, made default, tested; the problem shows inline.
			button(target, 'Add').click()
			await settle()
			expect(calls.map((c) => c.command)).toContain('add_detected_acp_agent')
			const test = calls.find((c) => c.command === 'test_acp_agent')
			expect(test?.args).toEqual({ id: 'agy' })
			const after = target.textContent ?? ''
			expect(after).toContain('Added')
			expect(after).toContain('Default')
			expect(after).toContain('not signed in')
			expect(after).toContain('Not signed in. Run `agy` once.')

			// Custom agent… opens the manual form.
			button(target, 'Custom agent…').click()
			await settle()
			expect(target.querySelector('#acp-new-command')).not.toBeNull()
		} finally {
			unmount(component)
			target.remove()
		}
	})
})
