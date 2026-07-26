# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Product documentation set (PRD, architecture, features, design language, kanban)
- Per-engine query policy SPI design (read default, write opt-in, DELETE blocked)
- V1 target engines: PostgreSQL, MongoDB, MySQL

### Changed

- (none yet for a release)

### Fixed

- (none yet for a release)

## [0.1.0] - 2026-07-26

Initial project baseline on `main`.

### Added

- Tauri 2 + SvelteKit desktop shell
- Design system primitives and home connection flow
- Local app SQLite for profiles, query library, history, settings, ER diagrams
- Connector API crate and plugin registry (Postgres, SQLite, Mongo pools)
- Engine session IPC (list / test / open / close / ping / pool status)
- Interactive FE ER designer (families, DBML, export PNG/PDF)
- Workspace shell layout (mock catalog/data where live engines not wired yet)

[Unreleased]: https://nova.teachx.ai/trace-analysis/mystique-db/-/compare/v0.1.0...main
[0.1.0]: https://nova.teachx.ai/trace-analysis/mystique-db/-/tags/v0.1.0
