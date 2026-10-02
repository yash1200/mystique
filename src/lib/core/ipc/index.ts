/**
 * Typed bridge to the Rust backend. Functions only — the DTOs they speak live
 * in `#core/contracts` and are generated from `mystique-connector-api`.
 */
export * from './engines'
export * from './menu'
export { isTauri } from './tauri'
