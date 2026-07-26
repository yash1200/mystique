# Product overview

**MystiqueDB** is a universal database studio for the desktop.

## Goals

- One app shell for multiple engines
- Engine-honest UX (SQL vs document, etc.)
- Connector-first architecture (new DB = plugin, not a fork)
- Safety by default in the query space

## V1 engines

- PostgreSQL  
- MongoDB  
- MySQL  

Other engines (e.g. SQLite) may exist as experiments or later targets.

## Main flows

1. Choose engine → save connection profile  
2. Open connection → workspace  
3. Explore objects, preview data, run queries, view model/ER  

## Query safety (summary)

| Mode | Behavior |
| ---- | -------- |
| Read (default) | Only read operations for that engine |
| Write (opt-in) | Limited writes per engine policy |
| DELETE | Blocked for now on all engines |

Each engine has its **own** query policy implementation (not one shared SQL filter for all databases).

## Full product docs (repo)

- [`docs/PRD.md`](../PRD.md)  
- [`docs/FEATURES.md`](../FEATURES.md)  
- [`docs/kanban.html`](../kanban.html)  
