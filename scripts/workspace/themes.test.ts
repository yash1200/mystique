import assert from 'node:assert/strict'
import { afterEach, test } from 'vitest'

import { appPrefs } from '../../src/lib/app/prefs.svelte'
import {
	paletteAttr,
	parseDarkTheme,
	parseLightTheme,
	themesFor,
} from '../../src/lib/app/themes'

afterEach(() => {
	appPrefs.setTheme('dark')
	appPrefs.setDarkTheme('night')
	appPrefs.setLightTheme('day')
	localStorage.clear()
})

test('two themes of each kind; unknown saved values fall back to the defaults', () => {
	assert.deepEqual(themesFor('dark').map((t) => t.id), ['night', 'abyss'])
	assert.deepEqual(themesFor('light').map((t) => t.id), ['day', 'linen'])
	assert.equal(parseDarkTheme('linen'), 'night')
	assert.equal(parseLightTheme(null), 'day')
	assert.equal(paletteAttr('night'), undefined)
	assert.equal(paletteAttr('abyss'), 'abyss')
})

test('the mode picks which of the two choices is on screen', () => {
	const root = document.documentElement
	appPrefs.setDarkTheme('abyss')
	appPrefs.setLightTheme('linen')

	appPrefs.setTheme('dark')
	assert.equal(root.dataset.theme, 'dark')
	assert.equal(root.dataset.palette, 'abyss')

	appPrefs.setTheme('light')
	assert.equal(root.dataset.theme, 'light')
	assert.equal(root.dataset.palette, 'linen')

	appPrefs.setLightTheme('day')
	assert.equal(root.dataset.palette, undefined, 'the defaults carry no palette')
	assert.equal(localStorage.getItem('mystique-light-theme'), 'day')
})
