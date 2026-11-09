//! The splitter corpus. `scripts/workspace/split-parity.test.ts` runs the
//! same files through the editor's TypeScript splitter.
//!
//! Each `<name>.<dialect>.sql` in `fixtures/split` has its statements listed
//! under its file name in `expected.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mystique_sql::{split_statement_spans, SqlDialect};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split")
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
fn corpus_splits_as_expected() {
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
        let want = expected
            .get(file)
            .unwrap_or_else(|| panic!("{file} has no entry in expected.json"));
        let spans = split_statement_spans(&sql, dialect_of(file));
        let got: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(&got, want, "{file}");
        for span in &spans {
            assert_eq!(&sql[span.start..span.end], span.text, "{file}: range");
        }
    }
}
