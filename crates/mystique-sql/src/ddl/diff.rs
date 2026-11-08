//! Same-engine schema diff: make one table list match another.

use std::collections::HashSet;

use super::order::order_changes;
use super::types::{
    ColumnDraft, ColumnOriginal, ForeignKeyDraft, ForeignKeyOriginal, IndexDraft, IndexOriginal,
    RenamePair, SchemaChange, SchemaMigration, TableDefinition,
};
use super::{plan_table_alter, DdlTarget};

/// Make `target` match `source`. Renames are only the pairs the user supplied.
pub fn plan_schema_diff(
    dialect: impl Into<DdlTarget>,
    source: &[TableDefinition],
    target: &[TableDefinition],
    renames: &[RenamePair],
) -> SchemaMigration {
    let dialect = dialect.into();
    let mut warnings = Vec::new();
    let mut used_source = HashSet::new();
    let mut used_target = HashSet::new();
    let mut drafts: Vec<TableDefinition> = Vec::new();

    for pair in renames {
        let from = source.iter().find(|table| table.name == pair.from);
        let to = target.iter().find(|table| table.name == pair.to);
        match (from, to) {
            (Some(from), Some(to)) => {
                if !used_source.insert(from.name.clone()) || !used_target.insert(to.name.clone()) {
                    warnings.push(format!(
                        "Rename pair {} → {} uses a table more than once.",
                        pair.from, pair.to
                    ));
                    continue;
                }
                let mut draft = align(from, to);
                if from.name != to.name {
                    draft.rename_to = Some(from.name.clone());
                }
                drafts.push(draft);
            }
            _ => warnings.push(format!(
                "Rename pair {} → {} does not match a table on both sides.",
                pair.from, pair.to
            )),
        }
    }

    for from in source {
        if used_source.contains(&from.name) {
            continue;
        }
        if let Some(to) = target.iter().find(|table| table.name == from.name) {
            used_source.insert(from.name.clone());
            used_target.insert(to.name.clone());
            drafts.push(align(from, to));
        }
    }

    for from in source {
        if used_source.contains(&from.name) || from.kind.eq_ignore_ascii_case("view") {
            continue;
        }
        let mut created = from.clone();
        created.create = true;
        created.drop_table = false;
        created.rename_to = None;
        created.schema_revision = None;
        created.original_validator = None;
        for column in &mut created.columns {
            column.original = None;
            column.drop = false;
        }
        for index in &mut created.indexes {
            index.original = None;
            index.drop = false;
        }
        for fk in &mut created.foreign_keys {
            fk.original = None;
            fk.drop = false;
        }
        drafts.push(created);
    }

    for to in target {
        if used_target.contains(&to.name) || to.kind.eq_ignore_ascii_case("view") {
            continue;
        }
        let mut dropped = to.clone();
        dropped.drop_table = true;
        dropped.create = false;
        drafts.push(dropped);
    }

    let mut changes: Vec<SchemaChange> = drafts
        .iter()
        .map(|draft| {
            let plan = plan_table_alter(dialect, draft);
            let action = if draft.drop_table {
                "drop"
            } else if draft.create {
                "create"
            } else if plan.statements.is_empty() {
                "unchanged"
            } else {
                "alter"
            };
            SchemaChange {
                name: draft.name.clone(),
                action: action.into(),
                plan,
                draft: draft.clone(),
            }
        })
        .collect();

    if let Err(message) = order_changes(&mut changes, &drafts) {
        warnings.push(message);
    }
    for change in &changes {
        if change.action != "unchanged" {
            warnings.extend(change.plan.warnings.clone());
        }
    }
    SchemaMigration { changes, warnings }
}

pub(super) fn align(desired: &TableDefinition, current: &TableDefinition) -> TableDefinition {
    let mut columns = Vec::new();
    for column in &desired.columns {
        if column.drop {
            continue;
        }
        let original = current.columns.iter().find(|item| item.name == column.name);
        columns.push(ColumnDraft {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            pk_position: column.pk_position,
            original: original.map(column_original),
            drop: false,
        });
    }
    for column in &current.columns {
        if desired.columns.iter().any(|item| item.name == column.name) {
            continue;
        }
        columns.push(ColumnDraft {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            pk_position: column.pk_position,
            original: Some(column_original(column)),
            drop: true,
        });
    }
    let mut indexes = Vec::new();
    for index in desired.indexes.iter().filter(|index| !index.primary) {
        let original = current.indexes.iter().find(|item| item.name == index.name);
        indexes.push(IndexDraft {
            name: index.name.clone(),
            columns: index.columns.clone(),
            unique: index.unique,
            primary: false,
            concurrent: false,
            original: original.map(index_original),
            drop: false,
        });
    }
    for index in current.indexes.iter().filter(|index| !index.primary) {
        if desired.indexes.iter().any(|item| item.name == index.name) {
            continue;
        }
        indexes.push(IndexDraft {
            name: index.name.clone(),
            columns: index.columns.clone(),
            unique: index.unique,
            primary: false,
            concurrent: false,
            original: Some(index_original(index)),
            drop: true,
        });
    }
    let mut foreign_keys = Vec::new();
    for fk in &desired.foreign_keys {
        let original = current
            .foreign_keys
            .iter()
            .find(|item| item.name == fk.name);
        foreign_keys.push(ForeignKeyDraft {
            name: fk.name.clone(),
            columns: fk.columns.clone(),
            ref_schema: fk.ref_schema.clone(),
            ref_table: fk.ref_table.clone(),
            ref_columns: fk.ref_columns.clone(),
            original: original.map(fk_original),
            drop: false,
        });
    }
    for fk in &current.foreign_keys {
        if desired.foreign_keys.iter().any(|item| item.name == fk.name) {
            continue;
        }
        foreign_keys.push(ForeignKeyDraft {
            name: fk.name.clone(),
            columns: fk.columns.clone(),
            ref_schema: fk.ref_schema.clone(),
            ref_table: fk.ref_table.clone(),
            ref_columns: fk.ref_columns.clone(),
            original: Some(fk_original(fk)),
            drop: true,
        });
    }
    TableDefinition {
        database: current.database.clone(),
        schema: current.schema.clone(),
        name: current.name.clone(),
        kind: current.kind.clone(),
        columns,
        indexes,
        foreign_keys,
        schema_revision: current.schema_revision.clone(),
        validator: desired.validator.clone(),
        original_validator: current.validator.clone(),
        create: false,
        drop_table: false,
        rename_to: None,
    }
}

pub(super) fn column_original(column: &ColumnDraft) -> ColumnOriginal {
    ColumnOriginal {
        name: column.name.clone(),
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        pk_position: column.pk_position,
        definition: column
            .original
            .as_ref()
            .and_then(|original| original.definition.clone()),
    }
}

pub(super) fn index_original(index: &IndexDraft) -> IndexOriginal {
    IndexOriginal {
        name: index.name.clone(),
        columns: index.columns.clone(),
        unique: index.unique,
        primary: index.primary,
    }
}

pub(super) fn fk_original(fk: &ForeignKeyDraft) -> ForeignKeyOriginal {
    ForeignKeyOriginal {
        name: fk.name.clone(),
        columns: fk.columns.clone(),
        ref_schema: fk.ref_schema.clone(),
        ref_table: fk.ref_table.clone(),
        ref_columns: fk.ref_columns.clone(),
    }
}
