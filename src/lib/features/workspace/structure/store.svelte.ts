/**
 * Loaded object structures, keyed by connectionId then tableId. Loading and
 * the last error are per connection too.
 */
import type { ObjectStructure } from '#core/contracts'
import { ConnectionCache } from '#features/workspace/session'

export type LiveStructure = { data: ObjectStructure; rev: number }

export const structureStore = new ConnectionCache<LiveStructure>()
