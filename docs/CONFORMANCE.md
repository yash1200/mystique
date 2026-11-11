# Plugin conformance kit

Every engine plugin runs one shared suite against a real server, or against a
temp file for SQLite. The suite checks the plugin contract that the rest of
Mystique relies on, not engine features. It answers one question: when a
plugin says it can do something, does it do it the way the app expects?

Code: `crates/mystique-conformance` (the kit, a dev-dependency only, never
shipped), `crates/plugins/*/tests/conformance.rs` (one suite per plugin), and
the `live` job in `.github/workflows/ci.yml`.

## The checks

The checks run in this order against a table (a collection, for documents)
called `mystique_conformance`. It holds 25 rows, with a key column holding
1 to 25 and a text column `name`. Each check has a 60 s limit, so a hung
check fails instead of stalling CI. When `setup` fails, the run stops there.

| Check | What has to hold |
|---|---|
| `descriptor` | The capabilities and descriptor carry the connector's id. At least one object kind is declared, and one is primary. Kind ids are non-empty lower case with no duplicates, and every editable kind is also browsable. Below `Operate` depth, server metrics and ER diagram are off. |
| `test_connection` | The connection test against the fixture config answers `ok`. |
| `ping` | The session reports the connector's engine id, and ping succeeds. |
| `setup` | The fixture's setup statements run, and the table then has 25 rows. |
| `list_catalog` | The full catalog lists the fixture table. |
| `list_catalog_children` | Walking root → database → schema → objects, one entry per page, follows the cursor and finds the table. |
| `describe_table` | The key column is there and is marked as the primary key. |
| `preview_pages` | Paging 7 rows at a time, by offset or page token, returns every row exactly once and never more than the page size. |
| `preview_sort` | A descending sort on the key starts at 25. |
| `preview_count` | The preview's total is 25. Skipped when the engine has no exact count. |
| `query_read` | The count statement runs in Read mode and returns 25. |
| `read_mode_enforced` | An insert sent straight to the plugin in Read mode fails, and the table still has 25 rows. The plugin or server must refuse it; the app's classifier is bypassed here on purpose. |
| `stream_rows` | Streaming the table yields 25 rows and at least 2 columns. |
| `cancel` | The fixture's slow statement, cancelled after 400 ms, ends in an error within 4 s, and the session still answers ping afterwards. Skipped when `supports_cancel_query` is off or the fixture has no slow statement. |
| `explain` | Explain on the count statement succeeds. Skipped when `supports_explain_plan` is off. |
| `pinned_transaction` | In a pinned session, an uncommitted insert is invisible from outside, rollback discards it and commit keeps it. Skipped when `supports_transactions` is off or the fixture has no SQL dialect. |
| `capability_honesty` | Each capability flag that is on has a method behind it that doesn't answer NotSupported. That covers distinct values, create statement (which must not be empty), view structure, server metrics, structure edit (table definition) and ER diagram (relationships). |
| `close` | The session closes cleanly, after teardown has dropped the table. |

A run never panics. It returns a `Report` that lists every check as `ok`,
`skip` (with the reason) or `FAIL` (with the reason). `report.assert_passed()`
fails the test with the whole report when any check failed.

## Engines and how to run them

| Engine | Runs where | Server from |
|---|---|---|
| SQLite | Ordinary `cargo test`, on every push (`rust` job) and in `live` | A temp file; nothing to set up |
| Postgres 16 | `live` job, or locally with `--ignored` | `MYSTIQUE_PG_{HOST,PORT,USER,PASSWORD,DATABASE}`. Defaults: `127.0.0.1:5432`, user `$USER`, no password, database `postgres`. |
| MySQL 8.4 and MariaDB 10.11 | `live` job (one suite, run twice), or locally with `--ignored` | `MYSTIQUE_MYSQL_{HOST,PORT,USER,PASSWORD,DATABASE}`. Defaults: `127.0.0.1:3306`, `admin` / `123`, database `mystique`, which must already exist. An empty `MYSTIQUE_MYSQL_PASSWORD` means no password. |
| MongoDB 7 | `live` job, or locally with `--ignored` | `MYSTIQUE_MONGO_URI`. Default: `mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin`. |

```sh
cargo test -p mystique-plugin-sqlite   --test conformance -- --nocapture
cargo test -p mystique-plugin-postgres --test conformance -- --ignored --nocapture
cargo test -p mystique-plugin-mysql    --test conformance -- --ignored --nocapture
cargo test -p mystique-plugin-mongodb  --test conformance -- --ignored --nocapture
```

`--nocapture` prints the report even when every check passes.

The Postgres and MySQL suites hold a second test as well. It creates one of
each extra object kind the plugin declares, then checks two things: that each
is listed under its kind, and that its definition can be opened.

- Postgres: materialized view, sequence, function with an overload,
  procedure and type.
- MySQL: function, procedure and trigger.

### Known gaps

- **MongoDB:** `pinned_transaction` is skipped, because the kit plans
  transactions as SQL batches. Mongo transactions are covered by
  `crates/plugins/mystique-plugin-mongodb/tests/pinned.rs`, which needs a
  replica set and isn't in the `live` job.
- **Older live tests:** the plugins' earlier `#[ignore]`d tests (for example
  Mongo's `tests/connect.rs` and `tests/pinned.rs`) hard-code a local server.
  They aren't in the `live` job until they read `MYSTIQUE_*` variables like the
  conformance suites do.

## Adding an engine

1. Add `mystique-conformance` as a dev-dependency of the plugin crate.
2. Write `tests/conformance.rs`. Build a `Fixture` with:
   - the connection config, read with `mystique_conformance::env_or` so CI
     and a laptop can point at different servers;
   - `database`, plus `schema` for engines that have schemas;
   - `key`: `id` for SQL, `_id` for documents;
   - `setup`: drop and recreate `TABLE` with `ROWS` rows, in the engine's own
     language;
   - `teardown`: drop it;
   - `count`: one statement that returns the row count as a single cell;
   - `insert`: one statement that inserts exactly one row, with key
     `ROWS + 1`;
   - `slow`: a statement that runs for at least 5 s, or `None` to skip
     `cancel`;
   - `dialect`: the SQL dialect for pinned batches, or `None` to skip
     `pinned_transaction`;
   - `collection`: for document engines, the collection queries run against.
3. Call `mystique_conformance::run(&YourConnector, &fixture).await` and then
   `report.assert_passed()`. Mark the test `#[ignore = "needs <engine>
   (MYSTIQUE_<ENGINE>_*)"]` unless it needs no server.
4. Add a service container and a step to the `live` job.
5. Add one test that creates each extra object kind the plugin declares, and
   check that it is listed and that its definition can be opened.

A check that is skipped must be skipped for a stated reason: a capability that
is off, or a fixture statement the engine can't express. If a check fails
because the engine really can't do the thing, turn its capability flag off.
Don't loosen the check.
