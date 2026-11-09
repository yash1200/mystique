/**
 * What a saved connection points at, worded for people: the fact grid on the
 * home detail panel and in the workspace info modal, and the copy-URL action
 * both of them (and the session rail menu) offer.
 */
import type { ConnectionProfile } from '#core/contracts'
import { errorMessage, formatEngineConnectionUrl } from '#core/ipc'
import { copyTextToClipboard } from '#core/util'
import {
	connectionKindFromCaps,
	databaseFromUri,
	defaultCapabilitiesForEngine,
} from '#engines'
import { showToast } from '#features/toast'
import {
	configBool,
	configNum,
	configStr,
	connectionEndpoint,
} from './model'

export type ConnectionFact = { label: string; value: string; wide?: boolean }

export function environmentLabel(env: ConnectionProfile['environment']): string {
	if (env === 'production') return 'Production'
	if (env === 'staging') return 'Staging'
	return 'Local'
}

export function formatConnectedAt(iso: string | null | undefined): string {
	if (!iso) return 'Never'
	try {
		return new Intl.DateTimeFormat(undefined, {
			dateStyle: 'medium',
			timeStyle: 'short',
		}).format(new Date(iso))
	} catch {
		return iso
	}
}

function connectionKind(profile: ConnectionProfile) {
	return connectionKindFromCaps(defaultCapabilitiesForEngine(profile.engine))
}

function sslLabel(profile: ConnectionProfile): string {
	const cfg = profile.pluginConfig ?? {}
	return configBool(cfg, 'ssl') || configStr(cfg, 'sslMode') === 'require'
		? 'Required'
		: 'Off'
}

/**
 * The fact grid. `full` (home) lists the target field by field; `summary`
 * (workspace modal) leaves the target to the URL line and skips empty fields.
 */
export function connectionFacts(
	profile: ConnectionProfile,
	mode: 'full' | 'summary',
): ConnectionFact[] {
	const cfg = profile.pluginConfig ?? {}
	const kind = connectionKind(profile)
	const items: ConnectionFact[] = []
	if (mode === 'full') {
		if (kind === 'uri') {
			items.push({ label: 'URI', value: configStr(cfg, 'uri', '—'), wide: true })
		} else if (kind === 'file') {
			items.push({ label: 'File', value: configStr(cfg, 'path', '—'), wide: true })
		} else {
			items.push(
				{ label: 'Host', value: configStr(cfg, 'host', '—') },
				{
					label: 'Port',
					value: configNum(cfg, 'port') ? String(configNum(cfg, 'port')) : '—',
				},
				{ label: 'Database', value: configStr(cfg, 'database', '—') },
				{ label: 'User', value: configStr(cfg, 'username', '—') },
				{
					label: 'Password',
					value: profile.passwordSet ? '••••••••' : 'Not set',
				},
				{ label: 'SSL', value: sslLabel(profile) },
			)
		}
		items.push({ label: 'Environment', value: environmentLabel(profile.environment) })
	} else {
		const database =
			configStr(cfg, 'database') ||
			databaseFromUri(profile.engine, configStr(cfg, 'uri')) ||
			''
		const username = configStr(cfg, 'username')
		if (database) items.push({ label: 'Database', value: database })
		if (username) items.push({ label: 'User', value: username })
		if (kind === 'host') items.push({ label: 'SSL', value: sslLabel(profile) })
	}
	items.push({
		label: 'Access',
		value: profile.readOnly ? 'Read-only' : 'Read / write',
	})
	items.push({
		label: 'Last connected',
		value: formatConnectedAt(profile.lastConnectedAt),
		// Full width when it would sit alone on the last row.
		wide: mode === 'full' || items.length % 2 === 0,
	})
	return items
}

function redactUrl(uri: string): string {
	try {
		const parsed = new URL(uri)
		if (parsed.password) parsed.password = '••••'
		return parsed.toString()
	} catch {
		return uri.replace(/:([^:@/]+)@/, ':••••@')
	}
}

/** One line for the target, password redacted. */
export function connectionUrlPreview(profile: ConnectionProfile): string {
	const cfg = profile.pluginConfig ?? {}
	const endpoint = connectionEndpoint(profile)
	const kind = connectionKind(profile)
	if (kind === 'file') return configStr(cfg, 'path') || endpoint
	if (kind === 'uri') return redactUrl(configStr(cfg, 'uri') || endpoint)
	const host = configStr(cfg, 'host')
	const port = configNum(cfg, 'port', 0)
	const user = configStr(cfg, 'username')
	const db = configStr(cfg, 'database')
	const hostPort = host ? (port ? `${host}:${port}` : host) : ''
	if (!hostPort) return endpoint
	return `${user ? `${user}@` : ''}${hostPort}${db ? `/${db}` : ''}`
}

/** Copy the engine-formatted connection URL and toast the outcome. */
export async function copyConnectionUrl(profile: ConnectionProfile): Promise<void> {
	try {
		const formatted = await formatEngineConnectionUrl({
			engine: String(profile.engine),
			pluginConfig: (profile.pluginConfig ?? {}) as Record<string, unknown>,
		})
		const url = formatted.url?.trim()
		if (!url) {
			showToast('warning', 'No connection URL available for this profile.')
			return
		}
		const result = await copyTextToClipboard(url)
		if (result.ok) {
			showToast('success', 'Connection URL copied.')
		} else if (!result.cancelled) {
			showToast('danger', result.error || 'Could not copy connection URL.')
		}
	} catch (err) {
		showToast('danger', errorMessage(err))
	}
}
