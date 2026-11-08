/**
 * `invoke` for every command. A rejection is always an `IpcError`, whatever
 * the backend or the bridge threw.
 */
import { invoke as tauriInvoke, type InvokeArgs } from '@tauri-apps/api/core'
import { toIpcError } from './error'

export { Channel } from '@tauri-apps/api/core'

export async function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
	try {
		return await tauriInvoke<T>(command, args)
	} catch (err) {
		throw toIpcError(err)
	}
}
