# Backend structure (Rust)

> Scalable layout for MystiqueDB desktop backend.

## Workspace

```text
mystique-db/
├── Cargo.toml                 # workspace root
├── crates/
│   └── mystique-core/         # domain + storage + services (no Tauri)
│       ├── migrations/
│       └── src/
│           ├── domain/        # pure types
│           ├── storage/       # SQLite (later: keychain, …)
│           └── application/   # use-case services
└── src-tauri/                 # thin desktop shell
    └── src/
        ├── lib.rs             # setup + manage AppServices
        └── ipc/               # Tauri commands → services
```

## Layer rules

| Layer | May depend on | Must not |
| ----- | ------------- | -------- |
| `domain` | serde only | rusqlite, tauri |
| `storage` | domain, rusqlite | tauri, UI types |
| `application` | domain, storage | tauri |
| `src-tauri/ipc` | mystique-core, tauri | SQL, business rules |

## Extension points

| Feature | Where |
| ------- | ----- |
| New engine plugin | `crates/mystique-plugin-<engine>` + registry in application |
| Keychain secrets | `storage/keychain` + hook in `ProfileService::save` |
| Schema change | `crates/mystique-core/migrations/00N_*.sql` |
| New IPC surface | `src-tauri/src/ipc/*.rs` thin adapter |

## Call path

```text
Svelte invoke("list_profiles")
  → ipc::profiles::list_profiles
  → AppServices.profiles().list()
  → AppDatabase.with_conn(repo::connections::list_all)
  → SQLite
```
