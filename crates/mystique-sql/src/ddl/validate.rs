//! Draft checks. Any warning blocks the plan.

use std::collections::HashSet;

use super::changes::{
    constraint_changes, default_changed, fk_changes, identity_changed, index_definition_changed,
};
use super::statements::retarget_index;
use super::types::TableDefinition;

/// `allow_untyped`: SQLite columns may have no declared type.
pub(super) fn validate_columns(
    draft: &TableDefinition,
    allow_untyped: bool,
    warnings: &mut Vec<String>,
) {
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
                    .trim()
                    .eq_ignore_ascii_case(column.data_type.trim()),
            };
        let untyped_ok = allow_untyped && column.data_type.trim().is_empty();
        if type_is_written && !untyped_ok && !type_is_safe(&column.data_type) {
            warnings.push(format!(
                "Column {name} has an unsupported type. Use letters, numbers, spaces, parentheses, brackets, dots, and commas."
            ));
        }
        let default_is_written = reprint_all
            || column
                .original
                .as_ref()
                .is_none_or(|original| default_changed(original, column));
        if default_is_written {
            if let Some(default) = column.default_value.as_deref() {
                if !expression_is_safe(default) {
                    warnings.push(format!(
                        "The default of column {name} must be one expression, without semicolons or comments."
                    ));
                }
            }
        }
        let identity_is_written = reprint_all
            || column
                .original
                .as_ref()
                .is_none_or(|original| identity_changed(original, column));
        if identity_is_written {
            if let Some(identity) = column.identity.as_deref() {
                let known = ["always", "by default", "auto_increment", "autoincrement"]
                    .iter()
                    .any(|word| identity.trim().eq_ignore_ascii_case(word));
                if !known {
                    warnings.push(format!("Column {name} has an unknown identity kind."));
                }
            }
        }
    }
    if live == 0 && !draft.drop_table && !draft.columns.is_empty() {
        warnings.push("A table needs at least one column.".into());
    }
}

pub(super) fn validate_indexes(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
    for index in &draft.indexes {
        if index.drop || index.primary {
            continue;
        }
        if index.name.trim().is_empty() {
            warnings.push("An index name is empty.".into());
        }
        if index.columns.is_empty() && index.definition.is_none() {
            warnings.push(format!("Index {} has no columns.", index.name));
        }
        if draft.kind.eq_ignore_ascii_case("collection") {
            continue;
        }
        // An index left alone follows column renames and drops on every
        // engine (SQLite: the rebuild replays or removes it).
        if index.original.is_some() && !index_definition_changed(index) {
            continue;
        }
        if index.original.is_none() {
            if let Some(definition) = index.definition.as_deref() {
                if retarget_index(definition, "t").is_err() {
                    warnings.push(format!(
                        "Index {} has a definition that is not one CREATE INDEX statement.",
                        index.name
                    ));
                }
                continue;
            }
        }
        for column in &index.columns {
            if !columns.iter().any(|name| name.eq_ignore_ascii_case(column)) {
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
        for action in [fk.on_delete.as_deref(), fk.on_update.as_deref()]
            .into_iter()
            .flatten()
        {
            let known = [
                "CASCADE",
                "SET NULL",
                "SET DEFAULT",
                "RESTRICT",
                "NO ACTION",
            ]
            .iter()
            .any(|word| action.trim().eq_ignore_ascii_case(word));
            if !known {
                warnings.push(format!(
                    "Foreign key {} has an unknown action {action}.",
                    fk.name
                ));
            }
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

pub(super) fn validate_constraints(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
    for constraint in &draft.constraints {
        if constraint.drop || (!draft.create && !constraint_changes(constraint)) {
            continue;
        }
        let label = if constraint.name.trim().is_empty() {
            constraint.kind.to_ascii_uppercase()
        } else {
            constraint.name.clone()
        };
        if constraint.kind.eq_ignore_ascii_case("check") {
            let expression = constraint.expression.as_deref().unwrap_or("").trim();
            if expression.is_empty() || !expression_is_safe(expression) {
                warnings.push(format!(
                    "Constraint {label} needs one CHECK expression, without semicolons or comments."
                ));
            }
        } else if constraint.kind.eq_ignore_ascii_case("unique") {
            if constraint.columns.is_empty() {
                warnings.push(format!("Constraint {label} has no columns."));
            }
            for column in &constraint.columns {
                if !columns.iter().any(|name| name.eq_ignore_ascii_case(column)) {
                    warnings.push(format!(
                        "Constraint {label} uses {column}, which is not a column on this table."
                    ));
                }
            }
        } else {
            warnings.push(format!(
                "Constraint {label} has an unknown kind {}.",
                constraint.kind
            ));
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

/// One SQL expression: balanced parentheses, closed quotes, and no `;` or
/// comment outside a string. It is spliced into DDL as written.
pub(super) fn expression_is_safe(expression: &str) -> bool {
    let text = expression.trim();
    if text.is_empty() {
        return false;
    }
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(open) = quote {
            if ch == open {
                if chars.peek() == Some(&open) {
                    chars.next();
                } else {
                    quote = None;
                }
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '[' => quote = Some(']'),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            ';' => return false,
            '-' if chars.peek() == Some(&'-') => return false,
            '/' if chars.peek() == Some(&'*') => return false,
            _ => {}
        }
    }
    quote.is_none() && depth == 0
}
