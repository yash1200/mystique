//! The placeholder corpus. `scripts/workspace/params-parity.test.ts` runs the
//! same files through the editor's TypeScript scanner.
//!
//! Each `<name>.<dialect>.sql` in `fixtures/params` has its parameter labels
//! listed under its file name in `expected.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mystique_sql::{ScriptParams, SqlDialect};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/params")
}

fn dialect_of(file: &str) -> SqlDialect {
    let stem = file.strip_suffix(".sql").expect("a .sql file");
    match stem.rsplit('.').next() {
        Some("postgres") => SqlDialect::Postgres,
        Some("mysql") => SqlDialect::Mysql,
        Some("sqlite") => SqlDialect::Sqlite,
        Some("generic") => SqlDialect::Generic,
        other => panic!("{file}: unknown dialect {other:?}"),
    }
}

#[test]
fn corpus_scans_as_expected() {
    let dir = corpus_dir();
    let expected: BTreeMap<String, Vec<String>> = serde_json::from_str(
        &fs::read_to_string(dir.join("expected.json")).expect("expected.json"),
    )
    .expect("expected.json parses");

    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("fixture dir")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".sql"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", dir.display());
    for name in expected.keys() {
        assert!(files.contains(name), "expected.json lists missing {name}");
    }

    for file in &files {
        let sql = fs::read_to_string(dir.join(file)).expect("fixture");
        let dialect = dialect_of(file);
        let want = expected
            .get(file)
            .unwrap_or_else(|| panic!("{file} has no entry in expected.json"));
        let scan = ScriptParams::scan(&sql, dialect).expect("scan");
        let got: Vec<&str> = scan.slots().iter().map(|s| s.label.as_str()).collect();
        assert_eq!(&got, want, "{file}");
        // The planner's offsets must still point at the typed text.
        assert_eq!(
            ScriptParams::mask(&sql, dialect).len(),
            sql.len(),
            "{file}: mask length"
        );
    }
}
