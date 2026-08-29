import type { CatalogPresentation, EngineCapabilities } from '$core/contracts'
import { defaultCapabilitiesForEngine, presentationFromCaps } from '$engines'
export function catalogPresentation(
	engine: string,
	caps?: EngineCapabilities | null,
): CatalogPresentation {
	return presentationFromCaps(caps ?? defaultCapabilitiesForEngine(engine))
}
