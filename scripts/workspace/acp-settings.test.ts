import { describe, expect, it } from 'vitest'
import type { AcpSettingsView } from '../../src/lib/core/contracts'
import {
	isExternalAgent,
	joinArgs,
	looksSecret,
	splitArgs,
} from '../../src/lib/features/workspace/agent/acp-settings.svelte'

function view(patch: Partial<AcpSettingsView>): AcpSettingsView {
	return {
		kind: 'builtin',
		command: '',
		args: [],
		env: [],
		agentLabel: 'External agent',
		...patch,
	}
}

describe('external agent settings', () => {
	it('runs the external agent only when it is picked and has a command', () => {
		expect(isExternalAgent(null)).toBe(false)
		expect(isExternalAgent(view({ kind: 'acp' }))).toBe(false)
		expect(isExternalAgent(view({ kind: 'builtin', command: 'npx' }))).toBe(false)
		expect(isExternalAgent(view({ kind: 'acp', command: 'npx' }))).toBe(true)
	})

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
})
