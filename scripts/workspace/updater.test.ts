import { flushSync, mount, tick, unmount } from 'svelte'
import { beforeAll, describe, expect, it, vi } from 'vitest'
import {
	releaseUrl,
	updateErrorMessage,
	type PendingUpdate,
	type UpdaterBackend,
} from '../../src/lib/features/updater/backend'
import UpdateNotice from '../../src/lib/features/updater/ui/UpdateNotice.svelte'
import { Updater, updater } from '../../src/lib/features/updater/updater.svelte'

function pending(patch: Partial<PendingUpdate> = {}): PendingUpdate {
	return {
		version: '1.0.3',
		currentVersion: '1.0.2',
		notes: 'Fixes.',
		date: null,
		async download(onProgress) {
			onProgress(0, 2_000_000)
			onProgress(1_000_000, 2_000_000)
			onProgress(2_000_000, 2_000_000)
		},
		install: vi.fn(async () => {}),
		...patch,
	}
}

function harness(check: UpdaterBackend['check']) {
	const notify = vi.fn()
	const relaunch = vi.fn(async () => {})
	const store = new Updater({ check, relaunch }, notify)
	return { store, notify, relaunch }
}

describe('update messages', () => {
	it('says what went wrong in one sentence', () => {
		expect(updateErrorMessage(new Error('error sending request for url'), 'check')).toContain(
			'Could not reach GitHub',
		)
		expect(
			updateErrorMessage('Could not fetch a valid release JSON from the remote', 'check'),
		).toBe('No update information is published for Mystique yet.')
		expect(updateErrorMessage('signature verification failed', 'download')).toContain(
			'signature check',
		)
		expect(updateErrorMessage({ weird: true }, 'download')).toBe(
			'The update could not be downloaded. Try again later.',
		)
		expect(releaseUrl('1.0.3')).toBe('https://github.com/yash1200/mystique/releases/tag/v1.0.3')
	})
})

describe('updater', () => {
	it('stays quiet on a launch check with nothing new', async () => {
		const { store, notify } = harness(async () => null)
		await store.check()
		expect(store.phase.kind).toBe('current')
		expect(store.noticeOpen).toBe(false)
		expect(store.checkedAt).not.toBeNull()
		expect(notify).not.toHaveBeenCalled()
	})

	it('answers a manual check, including when it fails', async () => {
		const ok = harness(async () => null)
		await ok.store.check({ manual: true })
		expect(ok.notify).toHaveBeenCalledWith('success', 'Mystique is up to date.')

		const down = harness(async () => {
			throw new Error('error sending request for url (https://github.com/…)')
		})
		await down.store.check()
		expect(down.notify).not.toHaveBeenCalled()
		await down.store.check({ manual: true })
		expect(down.notify).toHaveBeenCalledWith(
			'danger',
			'Could not reach GitHub. Check your internet connection and try again.',
		)
		expect(down.store.phase.kind).toBe('failed')
	})

	it('offers, downloads in the background, installs, then asks to restart', async () => {
		const update = pending()
		const { store, relaunch } = harness(async () => update)
		await store.check()
		expect(store.phase).toMatchObject({ kind: 'available', update: { version: '1.0.3' } })
		expect(store.noticeOpen).toBe(true)

		store.dismiss()
		expect(store.noticeOpen).toBe(false)

		await store.update()
		expect(update.install).toHaveBeenCalledOnce()
		expect(store.phase.kind).toBe('ready')
		expect(store.noticeOpen).toBe(true)

		// A manual check after the install only brings the prompt back.
		store.dismiss()
		await store.check({ manual: true })
		expect(store.phase.kind).toBe('ready')
		expect(store.noticeOpen).toBe(true)

		await store.restart()
		expect(relaunch).toHaveBeenCalledOnce()
	})

	it('reports download progress once per percent', async () => {
		let release!: () => void
		const shown = new Set<number>()
		let store!: Updater
		const update = pending({
			async download(onProgress) {
				// 1001 network chunks over 100 KB.
				for (let i = 0; i <= 1000; i++) {
					onProgress(i * 100, 100_000)
					const phase = store.phase
					if (phase.kind === 'downloading') shown.add(phase.received)
				}
				await new Promise<void>((r) => (release = r))
			},
		})
		store = harness(async () => update).store
		await store.check()
		const running = store.update()
		expect(store.busy).toBe(true)
		expect(store.phase).toMatchObject({ kind: 'downloading', received: 100_000, total: 100_000 })
		// 0 % through 100 %: one state change each, not one per chunk.
		expect(shown.size).toBe(101)
		release()
		await running
		expect(store.phase.kind).toBe('ready')
	})

	it('keeps a failed download retryable', async () => {
		let fail = true
		const update = pending({
			async download() {
				if (fail) throw new Error('error sending request')
			},
		})
		const { store } = harness(async () => update)
		await store.check()
		await store.update()
		expect(store.phase).toMatchObject({ kind: 'failed', update: { version: '1.0.3' } })
		expect(store.noticeOpen).toBe(true)
		fail = false
		await store.update()
		expect(store.phase.kind).toBe('ready')
	})
})

// happy-dom rejects `finished` when Svelte cancels a running transition.
beforeAll(() => {
	const animate = Element.prototype.animate
	Element.prototype.animate = function (...args: Parameters<typeof animate>) {
		const animation = animate.apply(this, args)
		animation.finished.catch(() => {})
		return animation
	}
})

async function settle() {
	for (let i = 0; i < 4; i++) {
		await tick()
		await new Promise((r) => setTimeout(r, 0))
	}
	flushSync()
}

describe('update notice', () => {
	it('shows each step with its actions', async () => {
		const info = { version: '1.0.3', currentVersion: '1.0.2', notes: null, date: null }
		const target = document.createElement('div')
		document.body.appendChild(target)
		const component = mount(UpdateNotice, { target })
		try {
			updater.phase = { kind: 'available', update: info }
			updater.noticeOpen = true
			await settle()
			let text = target.textContent ?? ''
			expect(text).toContain('Update available')
			expect(text).toContain('1.0.3 is available')
			expect(text).toContain('Not now')
			expect(target.querySelector('a')?.getAttribute('href')).toBe(releaseUrl('1.0.3'))

			updater.phase = { kind: 'downloading', update: info, received: 5 * 1024 * 1024, total: 10 * 1024 * 1024 }
			await settle()
			text = target.textContent ?? ''
			expect(text).toContain('5.0 of 10.0 MB')
			expect(target.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe('50')

			updater.phase = { kind: 'ready', update: info }
			await settle()
			expect(target.textContent).toContain('Restart now')

			const later = [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Later')!
			later.click()
			await settle()
			expect(updater.noticeOpen).toBe(false)
		} finally {
			unmount(component)
			target.remove()
			updater.phase = { kind: 'idle' }
		}
	})
})
