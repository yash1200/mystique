/**
 * Stand-ins for SvelteKit's `$app/*` modules, so a test can import a feature
 * barrel whose components read the page or navigate.
 */
export const page = {
	url: new URL('http://localhost/'),
	route: { id: null as string | null },
	params: {} as Record<string, string>,
}

export async function goto(_url: string | URL): Promise<void> {}

export function resolve(path: string): string {
	return path
}
