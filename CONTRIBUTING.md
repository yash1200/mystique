# Contributing to MystiqueDB

Thanks for helping improve MystiqueDB. This guide is intentionally general so it works for GitLab issues, merge requests, and day-to-day collaboration.

## Ways to contribute

- Report bugs and propose features via **GitLab Issues**
- Improve documentation under `docs/`
- Submit code changes via **Merge Requests (MRs)**
- Keep the kanban board in sync (`docs/kanban.html`, `docs/KANBAN.md`) when work moves

## Before you start

1. Read the [README](./README.md) for setup (Node, Rust, Tauri deps).
2. Skim product intent in [`docs/PRD.md`](./docs/PRD.md) and architecture in [`docs/ARCHITECTURE_AND_FEATURES.md`](./docs/ARCHITECTURE_AND_FEATURES.md).
3. Check open issues and the kanban board so you do not duplicate work.
4. For UI work, follow [`docs/DESIGN_LANGUAGE.md`](./docs/DESIGN_LANGUAGE.md).

## Development setup

```bash
git clone <repo-url>
cd mystique-db
yarn install          # or npm install
yarn tauri dev        # full desktop app (macOS / Linux / Windows)
# or
yarn dev              # frontend only (no Tauri APIs)
```

Windows: use PowerShell with Node, Yarn, and the MSVC Rust toolchain. You do not need WSL/bash for `yarn tauri` — scripts use `scripts/tauri.mjs`. Details and troubleshooting: [README](./README.md) and [docs/wiki/Development.md](./docs/wiki/Development.md). Related: GitLab [#69](https://nova.teachx.ai/trace-analysis/mystique-db/-/issues/69).

Typecheck:

```bash
yarn check
```

Rust workspace builds when you run Tauri; you can also build crates from the repo root with `cargo check` / `cargo test` as needed.

## GitLab workflow

### Issues

- One issue per problem or feature when practical.
- Use clear titles (e.g. `be: PostgresQueryPolicy rejects multi-statement`).
- Labels (when available): `FE`, `BE`, `DOCS`, `P0`–`P2`, and kanban columns if your project uses them.
- Link related tickets (`T-BE-003`, etc.) from the kanban when relevant.

### Branches

Prefer short, scoped branches from `main`:

```text
feat/<short-name>
fix/<short-name>
docs/<short-name>
chore/<short-name>
```

### Commits

- Prefer small, focused commits.
- Use conventional-style prefixes when possible: `feat:`, `fix:`, `docs:`, `chore:`, `test:`.
- Explain *why* in the body if the change is non-obvious.

### Merge requests

1. Open an MR against `main` (or the agreed target branch).
2. Fill in the MR template if the project provides one; otherwise include:
   - Summary of the change
   - How to test
   - Linked issue(s)
3. Keep MRs reviewable (prefer smaller diffs over mega-PRs).
4. Ensure the app still builds (`yarn check`, `yarn tauri build` or `cargo check` as appropriate).
5. Do not force-push to shared branches without agreement; force-push on your feature branch is fine if the team allows it.
6. Resolve discussion threads before merge when possible.

### Reviews

- Be kind and specific.
- Prefer questions and suggestions over drive-by rewrites.
- Approve only when you would ship the change.

## Project conventions (high level)

| Area | Expectation |
| ---- | ----------- |
| Backend | Plugin SPI in `mystique-connector-api`; drivers only in plugin crates |
| Query safety | Per-engine policy; default read; write opt-in; DELETE blocked for now |
| Frontend | Compose `$lib/components/ui/*`; design tokens only (no ad-hoc hex) |
| Secrets | Never commit credentials, tokens, or real connection passwords |
| Docs | Update PRD/architecture/kanban when product behavior changes |

## Code of conduct (short)

- Be respectful and constructive.
- Assume good intent; disagree on ideas, not people.
- No harassment, spam, or intentional sabotage of the project.

If something is unclear, open an issue or ask in the team channel before large unsolicited refactors.

## License

By contributing, you agree that your contributions are licensed under the same license as the project (see [LICENSE](./LICENSE)).
