/** Short developer asides shown while something is in flight. */

export const LOADING_PUNS = [
	'Blaming bad wifi.',
	'Compiling excuses.',
	'Asking Stack Overflow.',
	'Grepping the void.',
	'Checking git blame.',
	'Rebasing my patience.',
	'Clearing node_modules.',
	'Pinging localhost.',
	'SELECT * FROM patience.',
	'Consulting the rubber duck.',
	'Incrementing the timeout.',
	'Waiting on CI.',
	'Awaiting Promise.all.',
	'Unblocking the event loop.',
	'Warming the query cache.',
	'Scanning the full table.',
	'Holding a row lock.',
	'Vacuuming the table.',
	'Handshake in progress.',
	'Tail -f-ing hope.',
	'Reading the man page.',
	'Stepping through the debugger.',
	'Flushing the buffer.',
	'Fetching origin/patience.',
	'EXPLAIN ANALYZE-ing life.',
	'Deadlocking on purpose.',
	'SYN ACK-ing politely.',
	'npm installing patience.',
] as const

export function randomLoadingPun(): string {
	const i = Math.floor(Math.random() * LOADING_PUNS.length)
	return LOADING_PUNS[i] ?? LOADING_PUNS[0]
}
