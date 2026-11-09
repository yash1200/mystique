/**
 * Keep keyboard focus inside a dialog: Tab and Shift+Tab wrap around its
 * focusable elements, and focus that lands outside (a click on the page
 * behind, a stray programmatic focus) is pulled back in.
 *
 * Use as a Svelte action: `<div role="dialog" use:focusTrap>`. Returning
 * focus on close stays with whoever opened the dialog.
 */

const FOCUSABLE = [
	'a[href]',
	'button:not([disabled])',
	'input:not([disabled]):not([type="hidden"])',
	'select:not([disabled])',
	'textarea:not([disabled])',
	'[contenteditable="true"]',
	'[tabindex]',
].join(',')

/** Elements Tab can reach inside `root`, in document order. */
export function tabbables(root: ParentNode): HTMLElement[] {
	return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
		(el) => el.tabIndex >= 0 && !el.hidden && !el.closest('[inert]'),
	)
}

/**
 * Where Tab (or Shift+Tab with `back`) goes from `current`, wrapping at
 * the ends. Focus outside the list goes to the first (or last) element.
 */
export function nextTabStop(
	list: readonly HTMLElement[],
	current: Element | null,
	back: boolean,
): HTMLElement | null {
	if (list.length === 0) return null
	const at = current ? list.indexOf(current as HTMLElement) : -1
	if (at < 0) return (back ? list[list.length - 1] : list[0]) ?? null
	const next = back ? (at - 1 + list.length) % list.length : (at + 1) % list.length
	return list[next] ?? null
}

/**
 * `active` says whether the trap holds. Pass it when the dialog can be
 * closing while still in the DOM: restoring focus to the opener must not be
 * pulled back in.
 */
export function focusTrap(
	node: HTMLElement,
	active: () => boolean = () => true,
): { destroy(): void } {
	const onKeydown = (event: KeyboardEvent) => {
		if (event.key !== 'Tab' || event.defaultPrevented || !active()) return
		const target = nextTabStop(tabbables(node), document.activeElement, event.shiftKey)
		event.preventDefault()
		target?.focus()
	}
	const onFocusIn = (event: FocusEvent) => {
		if (!active()) return
		const target = event.target
		if (target instanceof Node && node.contains(target)) return
		tabbables(node)[0]?.focus()
	}
	node.addEventListener('keydown', onKeydown)
	document.addEventListener('focusin', onFocusIn)
	return {
		destroy() {
			node.removeEventListener('keydown', onKeydown)
			document.removeEventListener('focusin', onFocusIn)
		},
	}
}
