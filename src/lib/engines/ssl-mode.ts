export type SslMode =
	| 'disable'
	| 'prefer'
	| 'require'
	| 'verify-ca'
	| 'verify-full'

/** Require SSL must not stay paired with sslMode=disable. */
export function sslModeFromConfig(ssl: boolean, sslMode: string): SslMode {
	const mode = sslMode.trim().toLowerCase().replaceAll('_', '-')
	if (ssl && (mode === 'disable' || mode === '')) return 'require'
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

/** Sentence for a mode this engine will not open. Null when the mode is usable. */
export function sslModeRejection(engine: string, mode: SslMode): string | null {
	const id = engine.trim().toLowerCase()
	const label =
		id === 'mariadb'
			? 'MariaDB'
			: id === 'tidb'
				? 'TiDB'
				: id === 'documentdb'
					? 'DocumentDB'
					: id === 'mongodb' || id === 'mongo'
						? 'MongoDB'
						: id === 'mysql'
							? 'MySQL'
							: null
	const mongo = label === 'MongoDB' || label === 'DocumentDB'
	const mysql = label === 'MySQL' || label === 'MariaDB' || label === 'TiDB'
	if ((mongo || mysql) && mode === 'prefer' && label) {
		return `${label} cannot try TLS and then fall back to plaintext. Choose require or disable.`
	}
	if (mongo && mode === 'verify-ca' && label) {
		return `${label} verify-ca also checks the server hostname. Use verify-full with a CA file, or require for encryption only.`
	}
	return null
}
