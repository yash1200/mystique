//! What a draft changes compared with the table it was loaded from.

use super::types::{ColumnDraft, ForeignKeyDraft, TableDefinition};

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

pub(super) fn column_changes(column: &ColumnDraft) -> bool {
    if column.drop {
        return column.original.is_some();
    }
    match &column.original {
        None => true,
        Some(original) => {
            !original.name.eq_ignore_ascii_case(&column.name)
                || !original
                    .data_type
                    .eq_ignore_ascii_case(column.data_type.trim())
                || original.nullable != column.nullable
                || original.pk_position != column.pk_position
        }
    }
}

pub(super) fn fk_changes(fk: &ForeignKeyDraft) -> bool {
    if fk.drop {
        return fk.original.is_some();
    }
    match &fk.original {
        None => true,
        Some(original) => {
            original.columns != fk.columns
                || original.ref_table != fk.ref_table
                || original.ref_columns != fk.ref_columns
        }
    }
}

pub(super) fn has_structural_edit(draft: &TableDefinition) -> bool {
    draft.columns.iter().any(column_changes)
        || draft.indexes.iter().any(|index| {
            index.drop && index.original.is_some() || index.original.is_none() && !index.drop
        })
        || draft.foreign_keys.iter().any(fk_changes)
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
