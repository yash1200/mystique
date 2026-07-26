# MystiqueDB Wiki

Welcome to the **MystiqueDB** project wiki.

This wiki is meant for **general, living notes** that sit alongside the formal docs in the repository (`docs/PRD.md`, architecture, kanban). Keep pages short and link to the repo when detail already lives in git.

## Quick links

| Topic | Where |
| ----- | ----- |
| Setup & run | [README](../README.md) (repo root) |
| Contributing | [CONTRIBUTING](../CONTRIBUTING.md) |
| Changelog | [CHANGELOG](../CHANGELOG.md) |
| Product requirements | [PRD](./Product-Overview.md) → full text in `docs/PRD.md` |
| Architecture notes | [Architecture](./Architecture.md) → full text in `docs/ARCHITECTURE_AND_FEATURES.md` |
| Development tips | [Development](./Development.md) |
| FAQ | [FAQ](./FAQ.md) |

## What is MystiqueDB?

A multi-engine desktop database studio (Tauri + Svelte + Rust): connect to databases, explore structure and data, run queries in a **query space**, and use a **Model / ER** view. V1 targets **PostgreSQL**, **MongoDB**, and **MySQL**, with a **per-engine query policy** (read by default, write opt-in, DELETE blocked for now).

## GitLab

- **Issues** — bugs, features, discussion  
- **Merge requests** — code review  
- **Wiki** — this content (or a copy maintained in GitLab Wiki)  
- **Kanban** — open `docs/kanban.html` in a browser, or use the project issue board if configured  

## Editing this wiki

- Prefer small pages with clear titles.
- Do not duplicate long specs: link to files under `docs/` in the repository.
- When product decisions change, update the PRD/architecture in git *and* adjust wiki summaries if needed.
