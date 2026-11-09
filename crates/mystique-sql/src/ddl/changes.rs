//! What a draft changes compared with the table it was loaded from.

use super::sqlite_text::normalize_expression;
use super::statements::retarget_index;
use super::types::{
    ColumnDraft, ColumnOriginal, ConstraintDraft, ConstraintOriginal, ForeignKeyDraft, IndexDraft,
    TableDefinition,
};

pub(super) fn pk_changed(draft: &TableDefinition) -> bool {
    draft.columns.iter().any(|column| {
        column
            .original
            .as_ref()
            .is_some_and(|original| original.pk_position != column.pk_position && !column.drop)
            || (column.drop
                && column
                    .original
                    .as_ref()
                    .is_some_and(|original| original.pk_position > 0))
    })
}

/// A new column in the primary key also changes it.
pub(super) fn pk_changed_or_extended(draft: &TableDefinition) -> bool {
    pk_changed(draft)
        || draft
            .columns
            .iter()
            .any(|column| column.original.is_none() && !column.drop && column.pk_position > 0)
}

pub(super) fn type_changed(original: &ColumnOriginal, column: &ColumnDraft) -> bool {
    !original
        .data_type
        .trim()
        .eq_ignore_ascii_case(column.data_type.trim())
}

fn same_text(left: Option<&str>, right: Option<&str>) -> bool {
    let clean = |value: Option<&str>| {
        value
            .map(normalize_expression)
            .filter(|text| !text.is_empty())
    };
    clean(left) == clean(right)
}

pub(super) fn default_changed(original: &ColumnOriginal, column: &ColumnDraft) -> bool {
    !same_text(
        original.default_value.as_deref(),
        column.default_value.as_deref(),
    )
}

pub(super) fn identity_changed(original: &ColumnOriginal, column: &ColumnDraft) -> bool {
    let clean = |value: Option<&str>| {
        value
            .map(|text| text.trim().to_ascii_lowercase())
            .filter(|text| !text.is_empty())
    };
    clean(original.identity.as_deref()) != clean(column.identity.as_deref())
}

pub(super) fn column_changes(column: &ColumnDraft) -> bool {
    if column.drop {
        return column.original.is_some();
    }
    match &column.original {
        None => true,
        Some(original) => {
            !original.name.eq_ignore_ascii_case(&column.name)
                || type_changed(original, column)
                || original.nullable != column.nullable
                || original.pk_position != column.pk_position
                || default_changed(original, column)
                || identity_changed(original, column)
        }
    }
}

/// An existing FK whose definition differs from what was loaded.
pub(super) fn fk_definition_changed(fk: &ForeignKeyDraft) -> bool {
    fk.original.as_ref().is_some_and(|original| {
        !fk.drop
            && (!names_equal(&original.columns, &fk.columns)
                || !original.ref_table.eq_ignore_ascii_case(&fk.ref_table)
                || !names_equal(&original.ref_columns, &fk.ref_columns)
                || original.on_delete != fk.on_delete
                || original.on_update != fk.on_update)
    })
}

pub(super) fn fk_changes(fk: &ForeignKeyDraft) -> bool {
    if fk.drop {
        return fk.original.is_some();
    }
    fk.original.is_none() || fk_definition_changed(fk)
}

pub(super) fn index_definition_changed(index: &IndexDraft) -> bool {
    index.original.as_ref().is_some_and(|original| {
        original.unique != index.unique
            || !names_equal(&original.columns, &index.columns)
            || index_text_changed(index)
    })
}

/// The draft carries a different engine definition than the live index
/// (schema diff: a partial or expression index that changed).
pub(super) fn index_text_changed(index: &IndexDraft) -> bool {
    let Some(original) = &index.original else {
        return false;
    };
    match (index.definition.as_deref(), original.definition.as_deref()) {
        (Some(left), Some(right)) => {
            let clean = |text: &str| {
                normalize_expression(&retarget_index(text, "t").unwrap_or_else(|_| text.into()))
            };
            clean(left) != clean(right)
        }
        _ => false,
    }
}

pub(super) fn constraint_definition_changed(constraint: &ConstraintDraft) -> bool {
    constraint
        .original
        .as_ref()
        .is_some_and(|original| !constraint.drop && !constraint_same(original, constraint))
}

fn constraint_same(original: &ConstraintOriginal, constraint: &ConstraintDraft) -> bool {
    original.kind.eq_ignore_ascii_case(&constraint.kind)
        && names_equal(&original.columns, &constraint.columns)
        && same_text(
            original.expression.as_deref(),
            constraint.expression.as_deref(),
        )
}

pub(super) fn constraint_changes(constraint: &ConstraintDraft) -> bool {
    if constraint.drop {
        return constraint.original.is_some();
    }
    constraint.original.is_none() || constraint_definition_changed(constraint)
}

pub(super) fn names_equal(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.trim().eq_ignore_ascii_case(b.trim()))
}

pub(super) fn has_structural_edit(draft: &TableDefinition) -> bool {
    draft.columns.iter().any(column_changes)
        || draft.indexes.iter().any(|index| {
            index.drop && index.original.is_some() || index.original.is_none() && !index.drop
        })
        || draft.foreign_keys.iter().any(fk_changes)
        || draft.constraints.iter().any(constraint_changes)
}

pub(super) fn index_change_count(draft: &TableDefinition) -> usize {
    draft
        .indexes
        .iter()
        .filter(|index| {
            !index.primary && (index.drop || index.original.is_none() || index.concurrent)
        })
        .count()
}
