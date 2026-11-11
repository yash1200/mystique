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
	redactUrl,
} from './model'

export type ConnectionFact = {
	label: string
	value: string
	/** Spans the row in the two-column (`pairs`) grid. */
	wide?: boolean
	/** Data (hosts, users, times) is mono; words are not. Default true. */
	mono?: boolean
	/** A caution shown beside the value, in the warning tone. */
	warn?: string
}

export function environmentLabel(env: ConnectionProfile['environment']): string {
	if (env === 'production') return 'Production'
	if (env === 'staging') return 'Staging'
	return 'Local'
}

function formatConnectedAt(iso: string | null | undefined): string {
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

function sslOn(profile: ConnectionProfile): boolean {
	const cfg = profile.pluginConfig ?? {}
	return configBool(cfg, 'ssl') || configStr(cfg, 'sslMode') === 'require'
}

function passwordLabel(profile: ConnectionProfile): string {
	if (profile.passwordSet) return '••••••••'
	return profile.savePassword === false ? 'Asked on connect' : 'Not set'
}

function databaseOf(profile: ConnectionProfile): string {
	const cfg = profile.pluginConfig ?? {}
	return (
		configStr(cfg, 'database') ||
		databaseFromUri(profile.engine, configStr(cfg, 'uri')) ||
		''
	)
}

/**
 * The fact grid. `full` (home) lists everything but the target, which the
 * detail shows on its own line; `summary` (workspace modal) skips empty
 * fields.
 */
export function connectionFacts(
	profile: ConnectionProfile,
	mode: 'full' | 'summary',
): ConnectionFact[] {
	const cfg = profile.pluginConfig ?? {}
	const kind = connectionKind(profile)
	const items: ConnectionFact[] = []
	const access: ConnectionFact = {
		label: 'Access',
		value: profile.readOnly ? 'Read-only' : 'Read / write',
		mono: false,
	}
	if (mode === 'full') {
		const database = databaseOf(profile)
		if (kind === 'host') {
			const ssl = sslOn(profile)
			const port = configNum(cfg, 'port')
			items.push(
				{ label: 'Database', value: database || '—' },
				{ label: 'User', value: configStr(cfg, 'username', '—') || '—' },
				{ label: 'Password', value: passwordLabel(profile) },
				{ label: 'Port', value: port ? String(port) : '—' },
				{
					label: 'SSL',
					value: ssl ? 'Required' : 'Off',
					warn:
						!ssl && profile.environment === 'production'
							? 'Not encrypted'
							: undefined,
				},
			)
		} else if (kind === 'uri' && database) {
			items.push({ label: 'Database', value: database })
		}
		items.push(
			{
				label: 'Environment',
				value: environmentLabel(profile.environment),
				mono: false,
			},
			access,
			{ label: 'Last connected', value: formatConnectedAt(profile.lastConnectedAt) },
		)
		return items
	}
	const database = databaseOf(profile)
	const username = configStr(cfg, 'username')
	if (database) items.push({ label: 'Database', value: database })
	if (username) items.push({ label: 'User', value: username })
	if (kind === 'host') items.push({ label: 'SSL', value: sslOn(profile) ? 'Required' : 'Off' })
	items.push(access)
	items.push({
		label: 'Last connected',
		value: formatConnectedAt(profile.lastConnectedAt),
		// Full width when it would sit alone on the last row.
		wide: items.length % 2 === 0,
	})
	return items
}

/** Where the connection points: host:port, the file, or the URI (password redacted). */
export function connectionTarget(profile: ConnectionProfile): string {
	const cfg = profile.pluginConfig ?? {}
	const kind = connectionKind(profile)
	if (kind === 'file') return configStr(cfg, 'path') || '—'
	if (kind === 'uri') return redactUrl(configStr(cfg, 'uri')) || '—'
	const host = configStr(cfg, 'host')
	const port = configNum(cfg, 'port', 0)
	if (!host) return connectionEndpoint(profile)
	return port ? `${host}:${port}` : host
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
