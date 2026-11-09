//! Same-engine schema diff: make one table list match another.
//!
//! Tables pair by name or by an explicit rename pair. Columns pair by name,
//! by an explicit column pair, or by a guess (same type, and the same
//! position or a similar name) that the user can reject. Drops of any kind
//! are listed but applied only when the selection includes them.

use std::collections::HashSet;

use super::order::order_changes;
use super::sqlite_text::normalize_expression;
use super::statements::retarget_index;
use super::types::{
    ColumnDraft, ColumnOriginal, ColumnRename, ConstraintDraft, ConstraintOriginal, DiffSelection,
    DropItem, DropKey, ForeignKeyDraft, ForeignKeyOriginal, IndexDraft, IndexOriginal, RenamePair,
    SchemaChange, SchemaMigration, TableDefinition,
};
use super::{plan_table_alter, DdlTarget};

/// Make `target` match `source`, with the default selection: every create
/// and alter, no drops.
pub fn plan_schema_diff(
    dialect: impl Into<DdlTarget>,
    source: &[TableDefinition],
    target: &[TableDefinition],
    renames: &[RenamePair],
) -> SchemaMigration {
    plan_schema_diff_with(dialect, source, target, renames, &DiffSelection::default())
}

/// Make `target` match `source`. `renames` holds table pairs (`table` is
/// `None`) and column pairs; `selection` says which changes and drops apply.
pub fn plan_schema_diff_with(
    dialect: impl Into<DdlTarget>,
    source: &[TableDefinition],
    target: &[TableDefinition],
    renames: &[RenamePair],
    selection: &DiffSelection,
) -> SchemaMigration {
    let dialect = dialect.into();
    let mut warnings = Vec::new();
    let mut used_source = HashSet::new();
    let mut used_target = HashSet::new();
    let mut aligned: Vec<Aligned> = Vec::new();

    for pair in renames.iter().filter(|pair| pair.table.is_none()) {
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
                let mut result = align_with(from, to, renames, selection);
                if from.name != to.name {
                    result.draft.rename_to = Some(from.name.clone());
                }
                aligned.push(result);
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
            aligned.push(align_with(from, to, renames, selection));
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
        created.triggers.clear();
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
        for constraint in &mut created.constraints {
            constraint.original = None;
            constraint.drop = false;
        }
        aligned.push(Aligned::plain(created));
    }

    for to in target {
        if used_target.contains(&to.name) || to.kind.eq_ignore_ascii_case("view") {
            continue;
        }
        let mut dropped = to.clone();
        dropped.drop_table = true;
        dropped.create = false;
        let included = selection.drops.contains(&DropKey {
            table: to.name.clone(),
            kind: "table".into(),
            name: to.name.clone(),
        });
        let mut result = Aligned::plain(dropped);
        result.drops.push(DropItem {
            kind: "table".into(),
            name: to.name.clone(),
            included,
        });
        aligned.push(result);
    }

    let drafts: Vec<TableDefinition> = aligned.iter().map(|item| item.draft.clone()).collect();
    let mut changes: Vec<SchemaChange> = aligned
        .into_iter()
        .map(|item| {
            let draft = item.draft;
            let plan = plan_table_alter(dialect, &draft);
            let action = if draft.drop_table {
                "drop"
            } else if draft.create {
                "create"
            } else if plan.statements.is_empty() && plan.warnings.is_empty() {
                "unchanged"
            } else {
                "alter"
            };
            let selected = if draft.drop_table {
                item.drops.iter().any(|drop| drop.included)
            } else {
                !selection.skip.contains(&draft.name)
            };
            SchemaChange {
                name: draft.name.clone(),
                action: action.into(),
                plan,
                draft,
                selected,
                drops: item.drops,
                column_renames: item.column_renames,
            }
        })
        .collect();

    if let Err(message) = order_changes(&mut changes, &drafts) {
        warnings.push(message);
    }
    for change in &changes {
        if change.action != "unchanged" && change.selected {
            warnings.extend(change.plan.warnings.clone());
        }
    }
    SchemaMigration { changes, warnings }
}

/// One table's draft and what the diff found inside it.
struct Aligned {
    draft: TableDefinition,
    drops: Vec<DropItem>,
    column_renames: Vec<ColumnRename>,
}

impl Aligned {
    fn plain(draft: TableDefinition) -> Self {
        Self {
            draft,
            drops: Vec::new(),
            column_renames: Vec::new(),
        }
    }
}

fn similar_names(left: &str, right: &str) -> bool {
    let clean = |text: &str| {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let (a, b) = (clean(left), clean(right));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.contains(&b) || b.contains(&a) {
        return true;
    }
    let longest = a.chars().count().max(b.chars().count());
    edit_distance(&a, &b) * 3 <= longest
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == *cb {
                previous
            } else {
                1 + previous.min(above).min(row[j])
            };
            previous = above;
        }
    }
    row[b.len()]
}

/// Column pairs: `(desired index, current index, guessed)`.
fn pair_columns(
    desired: &TableDefinition,
    current: &TableDefinition,
    renames: &[RenamePair],
) -> Vec<(usize, usize, bool)> {
    let live: Vec<(usize, &ColumnDraft)> = desired
        .columns
        .iter()
        .enumerate()
        .filter(|(_, column)| !column.drop)
        .collect();
    let for_table: Vec<&RenamePair> = renames
        .iter()
        .filter(|pair| pair.table.as_deref() == Some(desired.name.as_str()))
        .collect();
    let mut pairs = Vec::new();
    let mut used_desired = HashSet::new();
    let mut used_current = HashSet::new();
    for pair in for_table.iter().filter(|pair| !pair.keep_apart) {
        let from = live.iter().find(|(_, column)| column.name == pair.from);
        let to = current
            .columns
            .iter()
            .enumerate()
            .find(|(_, column)| column.name == pair.to);
        if let (Some((d, _)), Some((c, _))) = (from, to) {
            if used_desired.insert(*d) && used_current.insert(c) {
                pairs.push((*d, c, false));
            }
        }
    }
    for (d, column) in &live {
        if used_desired.contains(d) {
            continue;
        }
        if let Some((c, _)) = current
            .columns
            .iter()
            .enumerate()
            .find(|(c, item)| !used_current.contains(c) && item.name == column.name)
        {
            used_desired.insert(*d);
            used_current.insert(c);
            pairs.push((*d, c, false));
        }
    }
    // Guess the rest: same type, and the same position or a similar name.
    let apart = |from: &str, to: &str| {
        for_table
            .iter()
            .any(|pair| pair.keep_apart && pair.from == from && pair.to == to)
    };
    let left: Vec<(usize, usize, &ColumnDraft)> = live
        .iter()
        .enumerate()
        .filter(|(_, (d, _))| !used_desired.contains(d))
        .map(|(position, (d, column))| (position, *d, *column))
        .collect();
    let right: Vec<(usize, &ColumnDraft)> = current
        .columns
        .iter()
        .enumerate()
        .filter(|(c, _)| !used_current.contains(c))
        .collect();
    let fits = |(position, _, d): &(usize, usize, &ColumnDraft),
                (c, column): &(usize, &ColumnDraft)| {
        d.data_type
            .trim()
            .eq_ignore_ascii_case(column.data_type.trim())
            && !apart(&d.name, &column.name)
            && (*position == *c || similar_names(&d.name, &column.name))
    };
    for item in &left {
        let candidates: Vec<&(usize, &ColumnDraft)> =
            right.iter().filter(|other| fits(item, other)).collect();
        let [only] = candidates.as_slice() else {
            continue;
        };
        let reverse = left.iter().filter(|other| fits(other, only)).count();
        if reverse == 1 && used_current.insert(only.0) {
            used_desired.insert(item.1);
            pairs.push((item.1, only.0, true));
        }
    }
    pairs
}

fn constraint_key(original: &ConstraintOriginal) -> String {
    let kind = original.kind.to_ascii_lowercase();
    if !original.name.trim().is_empty() {
        return format!("{kind}:{}", original.name.trim().to_ascii_lowercase());
    }
    match original.expression.as_deref() {
        Some(expression) if kind == "check" => {
            format!("check:({})", normalize_expression(expression))
        }
        _ => format!(
            "{kind}:[{}]",
            original
                .columns
                .iter()
                .map(|c| c.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// Index definitions compared without their table name and spacing.
fn same_index_definition(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let clean = |text: &str| {
                normalize_expression(&retarget_index(text, "t").unwrap_or_else(|_| text.into()))
            };
            clean(left) == clean(right)
        }
        _ => true,
    }
}

fn align_with(
    desired: &TableDefinition,
    current: &TableDefinition,
    renames: &[RenamePair],
    selection: &DiffSelection,
) -> Aligned {
    let wants_drop = |kind: &str, name: &str| {
        selection.drops.contains(&DropKey {
            table: current.name.clone(),
            kind: kind.into(),
            name: name.into(),
        })
    };
    let mut drops = Vec::new();
    let mut column_renames = Vec::new();

    let pairs = pair_columns(desired, current, renames);
    let mut columns = Vec::new();
    for (d, column) in desired.columns.iter().enumerate() {
        if column.drop {
            continue;
        }
        let pair = pairs.iter().find(|(pd, _, _)| *pd == d);
        let original = pair.map(|(_, c, _)| &current.columns[*c]);
        if let (Some(original), Some((_, _, guessed))) = (original, pair) {
            if original.name != column.name {
                column_renames.push(ColumnRename {
                    from: column.name.clone(),
                    to: original.name.clone(),
                    guessed: *guessed,
                });
            }
        }
        columns.push(ColumnDraft {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            pk_position: column.pk_position,
            original: original.map(column_original),
            drop: false,
            default_value: column.default_value.clone(),
            identity: column.identity.clone(),
        });
    }
    for (c, column) in current.columns.iter().enumerate() {
        if pairs.iter().any(|(_, pc, _)| *pc == c) {
            continue;
        }
        let included = wants_drop("column", &column.name);
        drops.push(DropItem {
            kind: "column".into(),
            name: column.name.clone(),
            included,
        });
        // A drop that is not chosen keeps the column as it is.
        columns.push(ColumnDraft {
            original: Some(column_original(column)),
            drop: included,
            ..column_as_is(column)
        });
    }

    let mut indexes = Vec::new();
    for index in desired.indexes.iter().filter(|index| !index.primary) {
        let original = current.indexes.iter().find(|item| item.name == index.name);
        let mut definition = index.definition.clone();
        if let Some(original) = original {
            if same_index_definition(definition.as_deref(), original.definition.as_deref()) {
                // Same index: keep the live text so nothing looks edited.
                definition.clone_from(&original.definition);
            }
        }
        indexes.push(IndexDraft {
            name: index.name.clone(),
            columns: index.columns.clone(),
            unique: index.unique,
            primary: false,
            concurrent: false,
            original: original.map(index_original),
            drop: false,
            definition,
        });
    }
    for index in current.indexes.iter().filter(|index| !index.primary) {
        if desired.indexes.iter().any(|item| item.name == index.name) {
            continue;
        }
        let included = wants_drop("index", &index.name);
        drops.push(DropItem {
            kind: "index".into(),
            name: index.name.clone(),
            included,
        });
        indexes.push(IndexDraft {
            original: Some(index_original(index)),
            drop: included,
            concurrent: false,
            ..index.clone()
        });
    }

    // A key into the source's own schema (MySQL: database) points at the
    // target's after the diff.
    let ref_schema = |schema: &Option<String>| {
        if schema.is_none() {
            None
        } else if *schema == desired.schema {
            current.schema.clone()
        } else if *schema == desired.database {
            current.database.clone()
        } else {
            schema.clone()
        }
    };
    let mut foreign_keys = Vec::new();
    for fk in &desired.foreign_keys {
        let original = current
            .foreign_keys
            .iter()
            .find(|item| item.name == fk.name);
        foreign_keys.push(ForeignKeyDraft {
            name: fk.name.clone(),
            columns: fk.columns.clone(),
            ref_schema: ref_schema(&fk.ref_schema),
            ref_table: fk.ref_table.clone(),
            ref_columns: fk.ref_columns.clone(),
            original: original.map(fk_original),
            drop: false,
            on_delete: fk.on_delete.clone(),
            on_update: fk.on_update.clone(),
        });
    }
    for fk in &current.foreign_keys {
        if desired.foreign_keys.iter().any(|item| item.name == fk.name) {
            continue;
        }
        let included = wants_drop("foreignKey", &fk.name);
        drops.push(DropItem {
            kind: "foreignKey".into(),
            name: fk.name.clone(),
            included,
        });
        foreign_keys.push(ForeignKeyDraft {
            original: Some(fk_original(fk)),
            drop: included,
            ..fk.clone()
        });
    }

    let mut constraints = Vec::new();
    let current_keys: Vec<String> = current
        .constraints
        .iter()
        .map(|item| constraint_key(&constraint_original(item)))
        .collect();
    let mut matched = HashSet::new();
    for constraint in &desired.constraints {
        let key = constraint_key(&constraint_original(constraint));
        let found = current_keys
            .iter()
            .position(|other| *other == key)
            .filter(|at| matched.insert(*at));
        constraints.push(ConstraintDraft {
            kind: constraint.kind.clone(),
            name: constraint.name.clone(),
            columns: constraint.columns.clone(),
            expression: constraint.expression.clone(),
            original: found.map(|at| constraint_original(&current.constraints[at])),
            drop: false,
        });
    }
    for (at, constraint) in current.constraints.iter().enumerate() {
        if matched.contains(&at) {
            continue;
        }
        let label = constraint_label(constraint);
        let included = wants_drop("constraint", &label);
        drops.push(DropItem {
            kind: "constraint".into(),
            name: label,
            included,
        });
        constraints.push(ConstraintDraft {
            original: Some(constraint_original(constraint)),
            drop: included,
            ..constraint.clone()
        });
    }

    let draft = TableDefinition {
        database: current.database.clone(),
        schema: current.schema.clone(),
        name: current.name.clone(),
        kind: current.kind.clone(),
        columns,
        indexes,
        foreign_keys,
        constraints,
        triggers: current.triggers.clone(),
        schema_revision: current.schema_revision.clone(),
        validator: desired.validator.clone(),
        original_validator: current.validator.clone(),
        create: false,
        drop_table: false,
        rename_to: None,
    };
    Aligned {
        draft,
        drops,
        column_renames,
    }
}

/// What a constraint is called in the drop list.
fn constraint_label(constraint: &ConstraintDraft) -> String {
    if !constraint.name.trim().is_empty() {
        return constraint.name.clone();
    }
    match constraint.expression.as_deref() {
        Some(expression) if constraint.kind.eq_ignore_ascii_case("check") => {
            format!("CHECK ({expression})")
        }
        _ => format!("UNIQUE ({})", constraint.columns.join(", ")),
    }
}

/// The column exactly as loaded, edits undone.
fn column_as_is(column: &ColumnDraft) -> ColumnDraft {
    let original = column_original(column);
    ColumnDraft {
        name: original.name.clone(),
        data_type: original.data_type.clone(),
        nullable: original.nullable,
        pk_position: original.pk_position,
        original: None,
        drop: false,
        default_value: original.default_value.clone(),
        identity: original.identity.clone(),
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
        default_value: column.default_value.clone(),
        identity: column.identity.clone(),
    }
}

pub(super) fn index_original(index: &IndexDraft) -> IndexOriginal {
    IndexOriginal {
        name: index.name.clone(),
        columns: index.columns.clone(),
        unique: index.unique,
        primary: index.primary,
        definition: index.definition.clone(),
    }
}

pub(super) fn fk_original(fk: &ForeignKeyDraft) -> ForeignKeyOriginal {
    ForeignKeyOriginal {
        name: fk.name.clone(),
        columns: fk.columns.clone(),
        ref_schema: fk.ref_schema.clone(),
        ref_table: fk.ref_table.clone(),
        ref_columns: fk.ref_columns.clone(),
        on_delete: fk.on_delete.clone(),
        on_update: fk.on_update.clone(),
    }
}

pub(super) fn constraint_original(constraint: &ConstraintDraft) -> ConstraintOriginal {
    ConstraintOriginal {
        kind: constraint.kind.clone(),
        name: constraint.name.clone(),
        columns: constraint.columns.clone(),
        expression: constraint.expression.clone(),
    }
}
