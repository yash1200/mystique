/**
 * The approval dock's state: the backend queue, re-read on every change
 * event. Nothing is kept here that the backend does not hold.
 */
import { errorMessage } from '#core/ipc'
import {
	decideToolApproval,
	listToolApprovals,
	listenToolApprovals,
	type ToolApprovalRequest,
} from './api'

class ApprovalQueue {
	pending = $state<ToolApprovalRequest[]>([])
	/** Ids being answered, so a double click sends one decision. */
	answering = $state<Record<string, boolean>>({})
	error = $state<string | null>(null)
	now = $state(Date.now())

	async refresh() {
		try {
			this.pending = await listToolApprovals()
			this.error = null
		} catch (e) {
			this.error = errorMessage(e)
		}
	}

	async decide(id: string, approve: boolean) {
		if (this.answering[id]) return
		this.answering = { ...this.answering, [id]: true }
		try {
			await decideToolApproval(id, approve)
			this.error = null
		} catch (e) {
			// Usually: it timed out a moment ago. The refresh drops it.
			this.error = errorMessage(e)
		} finally {
			const { [id]: _, ...rest } = this.answering
			this.answering = rest
			await this.refresh()
		}
	}

	/** Listen and tick the countdown. Returns the cleanup. */
	mount(): () => void {
		let stop: (() => void) | null = null
		let gone = false
		void listenToolApprovals(() => void this.refresh())
			.then((off) => (gone ? off() : (stop = off)))
			.catch(() => {
				/* not in the desktop shell */
			})
		void this.refresh()
		const tick = setInterval(() => {
			if (this.pending.length > 0) this.now = Date.now()
		}, 1000)
		return () => {
			gone = true
			stop?.()
			clearInterval(tick)
		}
	}
}

export const approvalQueue = new ApprovalQueue()
