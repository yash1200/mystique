/** True when running inside the Tauri webview (not plain browser `npm run dev`). */
export function isTauri(): boolean {
	return (
		typeof window !== 'undefined' &&
		('__TAURI_INTERNALS__' in window || '__TAURI__' in window)
	)
}
