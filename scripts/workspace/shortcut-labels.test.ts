import assert from 'node:assert/strict'
import { test } from 'vitest'

import { cmd, KEYMAP } from '../../src/lib/app/commands'
import {
	isSecretQuery,
	keyCap,
	shortcutGroups,
	shortcutMatches,
} from '../../src/lib/features/shortcuts/data'

test('glyphs on the Mac, words when asked; Ctrl stays a word elsewhere', () => {
	assert.equal(keyCap('Mod', 'glyphs', true), '⌘')
	assert.equal(keyCap('Shift', 'glyphs', true), '⇧')
	assert.equal(keyCap('Mod', 'words', true), 'Cmd')
	assert.equal(keyCap('Mod', 'glyphs', false), 'Ctrl')
	assert.equal(keyCap('Shift', 'glyphs', false), '⇧')
	assert.equal(keyCap('R', 'glyphs', true), 'R')
})

test('the filter matches the name or a key, written either way', () => {
	const item = { id: 'x', label: 'Reset UI scale', keys: ['Mod', '0'] }
	assert.equal(shortcutMatches(item, 'scale', true), true)
	assert.equal(shortcutMatches(item, '⌘', true), true)
	assert.equal(shortcutMatches(item, 'cmd', true), true)
	assert.equal(shortcutMatches(item, 'shift', true), false)
})

test('the pet shortcut is real but hidden from the list until searched for', () => {
	assert.ok(KEYMAP.includes(cmd.petMystique))
	const ids = shortcutGroups(true, KEYMAP).flatMap((g) => g.items.map((i) => i.id))
	assert.equal(ids.includes(cmd.petMystique.id), false)
	assert.equal(isSecretQuery('dog'), true)
	assert.equal(isSecretQuery('go'), false, 'three letters at least')
	assert.equal(isSecretQuery('good'), true)
	assert.equal(isSecretQuery('scale'), false)
})
