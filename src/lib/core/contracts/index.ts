/**
 * DTOs shared with the Rust side, generated into `core/ipc/bindings.gen.ts`
 * (`pnpm gen:ipc`) and re-exported here so feature code imports one surface.
 * The files in this folder keep only frontend shapes (catalog view models,
 * form drafts) and a few narrowings of generated types (`ConnectionProfile`,
 * `WriteGuardPrompt`), each documented where it is declared.
 *
 * Name clashes resolved here:
 * - `EngineFamily` is the plugin capability enum (`RELATIONAL` | …).
 *   The picker label type is `ConnectionFamily` (`relational` | `document`).
 */
export type * from './catalog'
export type * from './connection'
export type * from './engine-caps'
export type * from './engines'
export type * from './session'
export type * from './ddl'
export type * from './ipc'
export type * from './app'
