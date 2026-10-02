import type { ConnectionFormDraft, PluginConfig } from '#core/contracts'

export type SslMode =
	| 'disable'
	| 'prefer'
	| 'require'
	| 'verify-ca'
	| 'verify-full'

export const SSL_MODE_OPTIONS = [
	{ value: 'require', label: 'Require (encrypt)' },
	{ value: 'prefer', label: 'Prefer' },
	{ value: 'verify-ca', label: 'Verify CA' },
	{ value: 'verify-full', label: 'Verify full' },
]

export function sslModeFromConfig(
	ssl: boolean,
	sslMode: string,
): SslMode {
	const mode = sslMode.trim().toLowerCase().replaceAll('_', '-')
	if (
		mode === 'prefer' ||
		mode === 'require' ||
		mode === 'verify-ca' ||
		mode === 'verify-full'
	) {
		return mode
	}
	if (mode === 'disable') return 'disable'
	return ssl ? 'require' : 'disable'
}

export function sslPluginFields(draft: ConnectionFormDraft): PluginConfig {
	const mode = draft.ssl
		? sslModeFromConfig(true, draft.sslMode ?? 'require')
		: 'disable'
	const cfg: PluginConfig = {
		ssl: mode !== 'disable',
		sslMode: mode,
	}
	const ca = draft.caCertPath?.trim()
	if (ca) cfg.caCertPath = ca
	const cert = draft.clientCertPath?.trim()
	if (cert) cfg.clientCertPath = cert
	const key = draft.clientKeyPath?.trim()
	if (key) cfg.clientKeyPath = key
	return cfg
}

export const emptySslDraft = {
	ssl: false,
	sslMode: 'disable' as SslMode,
	caCertPath: '',
	clientCertPath: '',
	clientKeyPath: '',
	sslClientKeyPassphrase: '',
}
