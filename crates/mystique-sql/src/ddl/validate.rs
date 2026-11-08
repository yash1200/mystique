//! Draft checks. Any warning blocks the plan.

use std::collections::HashSet;

use super::changes::fk_changes;
use super::types::TableDefinition;

pub(super) fn validate_columns(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut live = 0usize;
    // Unchanged types are not spliced again. A CREATE reprints every type.
    let reprint_all = draft.create;
    for column in &draft.columns {
        if column.drop {
            continue;
        }
        live += 1;
        let name = column.name.trim();
        if name.is_empty() {
            warnings.push("A column name is empty.".into());
            continue;
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            warnings.push(format!("Column {name} is listed twice."));
        }
        let type_is_written = reprint_all
            || match &column.original {
                None => true,
                Some(original) => !original
                    .data_type
                    .eq_ignore_ascii_case(column.data_type.trim()),
            };
        if type_is_written && !type_is_safe(&column.data_type) {
            warnings.push(format!(
                "Column {name} has an unsupported type. Use letters, numbers, spaces, parentheses, brackets, dots, and commas."
            ));
        }
    }
    if live == 0 && !draft.drop_table && !draft.columns.is_empty() {
        warnings.push("A table needs at least one column.".into());
    }
}

pub(super) fn validate_indexes(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
    let renamed = renamed_column_names(draft);
    for index in &draft.indexes {
        if index.drop || index.primary {
            continue;
        }
        if index.name.trim().is_empty() {
            warnings.push("An index name is empty.".into());
        }
        if index.columns.is_empty() {
            warnings.push(format!("Index {} has no columns.", index.name));
        }
        if draft.kind.eq_ignore_ascii_case("collection") {
            continue;
        }
        // An index left alone follows a column rename on every engine.
        let unchanged = index.original.as_ref().is_some_and(|original| {
            original.columns == index.columns && original.unique == index.unique
        });
        for column in &index.columns {
            let known = columns.iter().any(|name| name.eq_ignore_ascii_case(column))
                || unchanged && renamed.iter().any(|name| name.eq_ignore_ascii_case(column));
            if !known {
                warnings.push(format!(
                    "Index {} uses {}, which is not a column on this table.",
                    index.name, column
                ));
            }
        }
    }
}

pub(super) fn validate_foreign_keys(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
    let renamed = renamed_column_names(draft);
    for fk in &draft.foreign_keys {
        if fk.drop {
            continue;
        }
        if fk.columns.len() != fk.ref_columns.len() || fk.columns.is_empty() {
            warnings.push(format!(
                "Foreign key {} must list the same number of columns on both sides.",
                fk.name
            ));
        }
        let unchanged = !fk_changes(fk);
        for column in &fk.columns {
            let known = columns.iter().any(|name| name.eq_ignore_ascii_case(column))
                || unchanged && renamed.iter().any(|name| name.eq_ignore_ascii_case(column));
            if !known {
                warnings.push(format!(
                    "Foreign key {} uses {}, which is not a column on this table.",
                    fk.name, column
                ));
            }
        }
    }
}

pub(super) fn live_column_names(draft: &TableDefinition) -> Vec<String> {
    draft
        .columns
        .iter()
        .filter(|column| !column.drop)
        .map(|column| column.name.clone())
        .collect()
}

/// Old names of live columns that are being renamed.
pub(super) fn renamed_column_names(draft: &TableDefinition) -> Vec<String> {
    draft
        .columns
        .iter()
        .filter(|column| !column.drop)
        .filter_map(|column| column.original.as_ref())
        .map(|original| original.name.clone())
        .collect()
}

pub(super) fn type_is_safe(data_type: &str) -> bool {
    let text = data_type.trim();
    !text.is_empty()
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '(' | ')' | ',' | '_' | '[' | ']' | '.')
        })
}
