import { expect, test } from 'vitest'
import {
	deleteProfile,
	listProfiles,
	saveProfile,
} from '../../src/lib/features/connections/api'
import { mockIpc } from './mock-ipc'

const row = {
	id: '7',
	name: 'local pg',
	engine: 'postgres',
	colorId: 'not-a-color',
	environment: 'production',
	readOnly: false,
	pluginConfig: null,
	password: 'leaked',
	passwordSet: 1,
}

test('profiles come from the backend, normalized and without a password', async () => {
	mockIpc({ list_profiles: () => [row] })
	const [profile] = await listProfiles()
	expect(profile).toMatchObject({
		id: '7',
		colorId: 'slate',
		environment: 'production',
		pluginConfig: {},
		password: null,
		passwordSet: true,
		savePassword: true,
		status: 'disconnected',
	})
})

test('save and delete go through the backend', async () => {
	const calls = mockIpc({
		save_profile: (args) => ({ ...row, ...(args.input as object) }),
		delete_profile: () => true,
	})
	const saved = await saveProfile({ name: 'renamed' } as never)
	expect(saved.name).toBe('renamed')
	await deleteProfile('7')
	expect(calls.map((c) => c.command)).toEqual(['save_profile', 'delete_profile'])
	expect(calls[1]!.args).toEqual({ id: '7' })
	await expect(deleteProfile(' ')).rejects.toThrow('required')
})

test('an unmocked command rejects', async () => {
	mockIpc({})
	await expect(listProfiles()).rejects.toMatchObject({ message: expect.stringContaining('list_profiles') })
})
