/**
 * Typed bridge to the Rust backend. Functions only — the DTOs they speak live
 * in `#core/contracts` and are generated from the Rust crates.
 *
 * Every command rejects with an `IpcError` (see `./error`).
 */
export * from './engines'
export * from './runs'
export * from './menu'
export * from './quit'
export * from './error'
export { invoke } from './invoke'
