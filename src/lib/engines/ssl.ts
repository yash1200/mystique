import type { ConnectionFormDraft, PluginConfig } from '#core/contracts'

export {
	sslModeFromConfig,
	sslModeRejection,
	type SslMode,
} from './ssl-mode'

import { sslModeFromConfig, type SslMode } from './ssl-mode'

export const SSL_MODE_OPTIONS = [
	{ value: 'require', label: 'Require (encrypt)' },
	{ value: 'prefer', label: 'Prefer' },
	{ value: 'verify-ca', label: 'Verify CA' },
	{ value: 'verify-full', label: 'Verify full' },
]

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
