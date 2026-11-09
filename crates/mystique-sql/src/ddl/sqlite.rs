//! SQLite: in-place ALTER where SQLite has one (add column, renames, index
//! changes); everything else rebuilds the table.
//!
//! The rebuild follows SQLite's "other kinds of table schema changes"
//! procedure. The new table is the original `sqlite_master.sql` text with
//! targeted edits, so whatever the draft does not touch (constraints,
//! COLLATE, CHECK, DEFAULT, generated columns, AUTOINCREMENT, `WITHOUT
//! ROWID`, `STRICT`) is kept as written. Indexes and triggers are replayed
//! from their own `sqlite_master.sql`. Column renames run first, in place,
//! so SQLite rewrites views and other tables' foreign keys itself.
//!
//! The plugin runs the statements in order and owns the steps marked in
//! [`steps`]: it checks the schema revision right after `BEGIN IMMEDIATE`,
//! turns `PRAGMA foreign_key_check` into a rollback when it finds rows, and
//! restores `PRAGMA foreign_keys` only if it was on.

use std::collections::HashSet;

use super::changes::{
    constraint_definition_changed, default_changed, fk_changes, fk_definition_changed,
    identity_changed, index_definition_changed, names_equal, pk_changed_or_extended, type_changed,
};
use super::sqlite_text::{
    check_expression, normalize_expression, Constraint, ConstraintKind, CreateTable, ForeignKeyRef,
    Item, Text, Tok,
};
use super::statements::{
    column_definition, constraint_body, create_index_sql, foreign_key_body, index_statements,
    qualify, quote, quote_list, rename_sql,
};
use super::types::{
    ColumnDraft, ConstraintDraft, ConstraintOriginal, DdlPlan, ForeignKeyOriginal, TableDefinition,
};
use super::{finish, DdlTarget};

/// Statements of a rebuild plan that the SQLite plugin handles itself.
pub mod steps {
    /// Before `BEGIN`: SQLite ignores this pragma inside a transaction.
    pub const FOREIGN_KEYS_OFF: &str = "PRAGMA foreign_keys = OFF";
    /// The plugin compares the live schema with the draft right after this.
    pub const BEGIN: &str = "BEGIN IMMEDIATE";
    /// Lets the old table's dependants point at the new one unchanged.
    pub const LEGACY_ALTER_ON: &str = "PRAGMA legacy_alter_table = ON";
    pub const LEGACY_ALTER_OFF: &str = "PRAGMA legacy_alter_table = OFF";
    /// Any row it returns rolls the rebuild back.
    pub const FOREIGN_KEY_CHECK: &str = "PRAGMA foreign_key_check";
    pub const COMMIT: &str = "COMMIT";
    /// Run only when foreign keys were on before the rebuild.
    pub const FOREIGN_KEYS_ON: &str = "PRAGMA foreign_keys = ON";
}

const S: DdlTarget = DdlTarget::SQLITE;

pub(super) fn plan_sqlite(draft: &TableDefinition) -> DdlPlan {
    if needs_rebuild(draft) {
        plan_rebuild(draft)
    } else {
        plan_in_place(draft)
    }
}

/// Changes SQLite's ALTER TABLE cannot make.
pub(super) fn needs_rebuild(draft: &TableDefinition) -> bool {
    let column_edit = draft.columns.iter().any(|column| match &column.original {
        Some(_) if column.drop => true,
        Some(original) => {
            type_changed(original, column)
                || original.nullable != column.nullable
                || default_changed(original, column)
                || identity_changed(original, column)
        }
        None => !column.drop && column.identity.is_some(),
    });
    column_edit
        || pk_changed_or_extended(draft)
        || draft.foreign_keys.iter().any(fk_changes)
        || draft
            .constraints
            .iter()
            .any(super::changes::constraint_changes)
}

fn not_null_warning(column: &ColumnDraft) -> Option<String> {
    let has_default = column
        .default_value
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty());
    (!column.nullable && !has_default).then(|| {
        format!(
            "Add column {} as nullable, or give it a default. SQLite cannot add a NOT NULL column without one.",
            column.name
        )
    })
}

fn plan_in_place(draft: &TableDefinition) -> DdlPlan {
    let mut warnings = Vec::new();
    let mut statements = Vec::new();
    let table = qualify(S, draft);
    for column in draft.columns.iter().filter(|column| !column.drop) {
        match &column.original {
            None => {
                warnings.extend(not_null_warning(column));
                statements.push(format!(
                    "ALTER TABLE {table} ADD COLUMN {}",
                    column_definition(S, column)
                ));
            }
            Some(original) if !original.name.eq_ignore_ascii_case(&column.name) => {
                statements.push(format!(
                    "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                    quote(S, &original.name),
                    quote(S, &column.name)
                ));
            }
            Some(_) => {}
        }
    }
    statements.extend(index_statements(S, draft, true));
    if let Some(rename) = rename_sql(S, draft) {
        statements.push(rename);
    }
    // SQLite DDL is transactional: a failed step leaves nothing half-applied.
    let transactional = statements.len() > 1;
    if transactional {
        statements.insert(0, "BEGIN".into());
        statements.push("COMMIT".into());
    }
    finish(S, statements, warnings, transactional, false, None)
}

/// Renamed and dropped columns of the draft, by their original names.
struct ColumnMoves {
    renames: Vec<(String, String)>,
    dropped: Vec<String>,
}

impl ColumnMoves {
    fn of(draft: &TableDefinition) -> Self {
        let mut renames = Vec::new();
        let mut dropped = Vec::new();
        for column in &draft.columns {
            let Some(original) = &column.original else {
                continue;
            };
            if column.drop {
                dropped.push(original.name.clone());
            } else if original.name != column.name.trim() {
                renames.push((original.name.clone(), column.name.trim().to_string()));
            }
        }
        Self { renames, dropped }
    }

    fn new_name(&self, old: &str) -> Option<&str> {
        self.renames
            .iter()
            .find(|(from, _)| from.eq_ignore_ascii_case(old))
            .map(|(_, to)| to.as_str())
    }

    fn is_dropped(&self, name: &str) -> bool {
        self.dropped
            .iter()
            .any(|dropped| dropped.eq_ignore_ascii_case(name))
    }

    /// The first dropped column mentioned in `first..=last`.
    fn dropped_in(&self, text: &Text<'_>, first: usize, last: usize) -> Option<String> {
        if first > last {
            return None;
        }
        text.column_refs(first, last)
            .into_iter()
            .map(|(_, name)| name)
            .find(|name| self.is_dropped(name))
    }
}

/// Key that matches a CHECK / UNIQUE in the text with the draft's original.
fn constraint_key(kind: &str, name: &str, columns: &[String], expression: Option<&str>) -> String {
    let name = name.trim();
    if !name.is_empty() {
        return format!("{kind}:name:{}", name.to_ascii_lowercase());
    }
    if kind == "check" {
        format!(
            "check:expr:{}",
            normalize_expression(expression.unwrap_or(""))
        )
    } else {
        let cols: Vec<String> = columns.iter().map(|c| c.to_ascii_lowercase()).collect();
        format!("unique:cols:{}", cols.join(","))
    }
}

fn original_key(original: &ConstraintOriginal) -> String {
    let kind = original.kind.to_ascii_lowercase();
    constraint_key(
        &kind,
        &original.name,
        &original.columns,
        original.expression.as_deref(),
    )
}

fn fk_matches(found: &ForeignKeyRef, name: Option<&str>, original: &ForeignKeyOriginal) -> bool {
    if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
        if name.eq_ignore_ascii_case(&original.name) {
            return true;
        }
    }
    names_equal(&found.columns, &original.columns)
        && found.table.eq_ignore_ascii_case(&original.ref_table)
        && (found.ref_columns.is_empty() || names_equal(&found.ref_columns, &original.ref_columns))
}

/// What the rebuild does with one constraint of the original text.
enum Fate {
    Keep,
    /// The draft drops or replaces it.
    Remove,
}

struct Rebuild<'a, 'd> {
    draft: &'d TableDefinition,
    table: CreateTable<'a>,
    moves: ColumnMoves,
    pk_rewrite: bool,
    /// The single new primary key column written as `PRIMARY KEY AUTOINCREMENT`.
    inline_pk: Option<String>,
    warnings: Vec<String>,
    notes: Vec<String>,
    /// Draft keys and FKs matched in the text.
    matched_constraints: HashSet<usize>,
    matched_fks: HashSet<usize>,
}

impl<'a, 'd> Rebuild<'a, 'd> {
    fn text(&self) -> &Text<'a> {
        &self.table.text
    }

    fn draft_column(&self, name: &str) -> Option<&'d ColumnDraft> {
        self.draft.columns.iter().find(|column| {
            column
                .original
                .as_ref()
                .is_some_and(|original| original.name.eq_ignore_ascii_case(name))
        })
    }

    /// Token indexes inside `constraint` that name a renamed column.
    fn renamed_tokens(&self, constraint: &Constraint) -> Vec<usize> {
        let text = self.text();
        let self_table = self.draft.name.trim();
        let ranges: Vec<(usize, usize)> = match &constraint.kind {
            ConstraintKind::Check { open, close } | ConstraintKind::Generated { open, close } => {
                vec![(open + 1, close.saturating_sub(1))]
            }
            ConstraintKind::PrimaryKey { .. } | ConstraintKind::Unique { .. } => {
                let start = if constraint.name.is_some() {
                    constraint.first + 2
                } else {
                    constraint.first
                };
                vec![(start, constraint.last)]
            }
            ConstraintKind::ForeignKey(fk) => {
                let mut ranges = Vec::new();
                if let Some((open, close)) = fk.column_group {
                    ranges.push((open + 1, close.saturating_sub(1)));
                }
                if fk.table.eq_ignore_ascii_case(self_table) {
                    if let Some((open, close)) = fk.ref_group {
                        ranges.push((open + 1, close.saturating_sub(1)));
                    }
                }
                ranges
            }
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for (first, last) in ranges {
            if first > last {
                continue;
            }
            for (at, name) in text.column_refs(first, last) {
                if self.moves.new_name(&name).is_some() {
                    out.push(at);
                }
            }
        }
        out
    }

    /// Constraint text with renamed columns, or a part of it.
    fn render(&self, constraint: &Constraint, last: usize) -> String {
        let text = self.text();
        let tokens = self.renamed_tokens(constraint);
        text.render(constraint.first, last, &|at| {
            if tokens.contains(&at) {
                text.ident(at)
                    .and_then(|name| self.moves.new_name(&name).map(|to| quote(S, to)))
            } else {
                None
            }
        })
    }

    /// Does the draft drop or replace this CHECK / UNIQUE?
    fn constraint_fate(&mut self, key: &str) -> Fate {
        let found = self
            .draft
            .constraints
            .iter()
            .enumerate()
            .find(|(_, draft)| {
                draft
                    .original
                    .as_ref()
                    .is_some_and(|o| original_key(o) == key)
            });
        match found {
            Some((index, draft)) => {
                self.matched_constraints.insert(index);
                let renamed = draft
                    .original
                    .as_ref()
                    .is_some_and(|o| o.name.trim() != draft.name.trim());
                if draft.drop || constraint_definition_changed(draft) || renamed {
                    Fate::Remove
                } else {
                    Fate::Keep
                }
            }
            None => Fate::Keep,
        }
    }

    fn fk_fate(&mut self, fk: &ForeignKeyRef, name: Option<&str>) -> Fate {
        let found = self
            .draft
            .foreign_keys
            .iter()
            .enumerate()
            .find(|(index, draft)| {
                !self.matched_fks.contains(index)
                    && draft
                        .original
                        .as_ref()
                        .is_some_and(|original| fk_matches(fk, name, original))
            });
        match found {
            Some((index, draft)) => {
                self.matched_fks.insert(index);
                if draft.drop || fk_definition_changed(draft) {
                    Fate::Remove
                } else {
                    Fate::Keep
                }
            }
            None => Fate::Keep,
        }
    }

    /// Keep, edit, or drop one constraint. `None` drops it.
    fn constraint_text(
        &mut self,
        constraint: &Constraint,
        column: Option<&ColumnDraft>,
    ) -> Option<String> {
        let text_last = constraint.last;
        match &constraint.kind {
            ConstraintKind::PrimaryKey {
                columns,
                autoincrement,
            } => {
                if self.pk_rewrite {
                    return None;
                }
                if let Some(missing) = columns.iter().find(|name| self.moves.is_dropped(name)) {
                    self.warnings.push(format!(
                        "The primary key uses dropped column {missing}. Change the primary key too."
                    ));
                    return None;
                }
                let identity = column.and_then(|c| {
                    c.original
                        .as_ref()
                        .filter(|original| identity_changed(original, c))
                        .map(|_| c.identity.is_some())
                });
                match (identity, autoincrement) {
                    (Some(false), Some(at)) => Some(self.render(constraint, at - 1)),
                    (Some(true), None) => Some(format!(
                        "{} AUTOINCREMENT",
                        self.render(constraint, text_last)
                    )),
                    _ => Some(self.render(constraint, text_last)),
                }
            }
            ConstraintKind::NotNull => {
                (!column.is_some_and(|c| c.nullable)).then(|| self.render(constraint, text_last))
            }
            ConstraintKind::Null => column
                .is_none_or(|c| c.nullable)
                .then(|| self.render(constraint, text_last)),
            ConstraintKind::Default => {
                let changed = column.is_some_and(|c| {
                    c.original
                        .as_ref()
                        .is_some_and(|original| default_changed(original, c))
                });
                (!changed).then(|| self.render(constraint, text_last))
            }
            ConstraintKind::Check { open, close } => {
                let expression = check_expression(self.text(), *open, *close);
                let key = constraint_key(
                    "check",
                    constraint.name.as_deref().unwrap_or(""),
                    &[],
                    Some(&expression),
                );
                if let Fate::Remove = self.constraint_fate(&key) {
                    return None;
                }
                if let Some(dropped) = self.moves.dropped_in(self.text(), open + 1, *close) {
                    self.notes.push(format!(
                        "CHECK ({expression}) uses dropped column {dropped} and is removed."
                    ));
                    return None;
                }
                Some(self.render(constraint, text_last))
            }
            ConstraintKind::Unique { columns } => {
                let key = constraint_key(
                    "unique",
                    constraint.name.as_deref().unwrap_or(""),
                    columns,
                    None,
                );
                if let Fate::Remove = self.constraint_fate(&key) {
                    return None;
                }
                if let Some(dropped) = columns.iter().find(|name| self.moves.is_dropped(name)) {
                    self.notes.push(format!(
                        "UNIQUE ({}) uses dropped column {dropped} and is removed.",
                        columns.join(", ")
                    ));
                    return None;
                }
                Some(self.render(constraint, text_last))
            }
            ConstraintKind::ForeignKey(fk) => {
                if let Fate::Remove = self.fk_fate(fk, constraint.name.as_deref()) {
                    return None;
                }
                if let Some(dropped) = fk.columns.iter().find(|name| self.moves.is_dropped(name)) {
                    self.notes.push(format!(
                        "The foreign key on {} uses dropped column {dropped} and is removed.",
                        fk.columns.join(", ")
                    ));
                    return None;
                }
                Some(self.render(constraint, text_last))
            }
            ConstraintKind::Collate => Some(self.render(constraint, text_last)),
            ConstraintKind::Generated { open, close } => {
                if let Some(dropped) = self.moves.dropped_in(self.text(), open + 1, *close) {
                    self.warnings.push(format!(
                        "A generated column uses dropped column {dropped}. Drop or change it first."
                    ));
                }
                Some(self.render(constraint, text_last))
            }
        }
    }
}

fn plan_rebuild(draft: &TableDefinition) -> DdlPlan {
    let blocked = |warning: String| finish(S, Vec::new(), vec![warning], false, false, None);
    let Some(revision) = draft
        .schema_revision
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return blocked(format!(
            "Reload table {}. SQLite rebuilds it from its CREATE TABLE text, and the draft does not have it.",
            draft.name
        ));
    };
    let table = match CreateTable::parse(revision) {
        Ok(table) => table,
        Err(err) => {
            return blocked(format!(
                "Table {} cannot be rebuilt here: Mystique could not read its CREATE TABLE. {err}",
                draft.name
            ));
        }
    };
    let moves = ColumnMoves::of(draft);
    let pk_columns = {
        let mut pk: Vec<&ColumnDraft> = draft
            .columns
            .iter()
            .filter(|column| !column.drop && column.pk_position > 0)
            .collect();
        pk.sort_by_key(|column| column.pk_position);
        pk
    };
    let identity_added_without_inline_pk = draft.columns.iter().any(|column| {
        !column.drop
            && column.identity.is_some()
            && column
                .original
                .as_ref()
                .is_none_or(|original| original.identity.is_none())
            && !table.columns().any(|def| {
                column
                    .original
                    .as_ref()
                    .is_some_and(|original| def.name.eq_ignore_ascii_case(&original.name))
                    && def
                        .constraints
                        .iter()
                        .any(|c| matches!(c.kind, ConstraintKind::PrimaryKey { .. }))
            })
    });
    let pk_rewrite = pk_changed_or_extended(draft) || identity_added_without_inline_pk;
    let mut rebuild = Rebuild {
        draft,
        table,
        moves,
        pk_rewrite,
        inline_pk: None,
        warnings: Vec::new(),
        notes: Vec::new(),
        matched_constraints: HashSet::new(),
        matched_fks: HashSet::new(),
    };
    if pk_rewrite {
        let identity: Vec<&ColumnDraft> = draft
            .columns
            .iter()
            .filter(|c| !c.drop && c.identity.is_some())
            .collect();
        if !identity.is_empty() {
            if pk_columns.len() == 1
                && identity.len() == 1
                && std::ptr::eq(pk_columns[0], identity[0])
            {
                rebuild.inline_pk = Some(pk_columns[0].name.trim().to_string());
            } else {
                rebuild
                    .warnings
                    .push("AUTOINCREMENT needs the primary key to be one INTEGER column.".into());
            }
        }
        if pk_columns.is_empty() && rebuild.table.without_rowid() {
            rebuild
                .warnings
                .push("A WITHOUT ROWID table needs a primary key.".into());
        }
    }
    for column in draft
        .columns
        .iter()
        .filter(|c| !c.drop && c.identity.is_some())
    {
        if !column.data_type.trim().eq_ignore_ascii_case("INTEGER") {
            rebuild.warnings.push(format!(
                "AUTOINCREMENT on column {} needs the type INTEGER.",
                column.name
            ));
        }
    }
    let known: Vec<String> = rebuild
        .table
        .columns()
        .map(|def| def.name.clone())
        .collect();
    for column in &draft.columns {
        if let Some(original) = &column.original {
            if !known
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&original.name))
            {
                rebuild.warnings.push(format!(
                    "Column {} is not in the CREATE TABLE text. Reload the table.",
                    original.name
                ));
            }
        }
    }
    // Triggers are replayed as written. A rename would leave them stale.
    if !rebuild.moves.renames.is_empty() {
        for trigger in &draft.triggers {
            let Ok(text) = Text::new(&trigger.sql) else {
                continue;
            };
            let last = text.len().saturating_sub(1);
            if let Some((old, _)) = rebuild
                .moves
                .renames
                .iter()
                .find(|(old, _)| text.len() > 0 && text.mentions(0, last, old))
            {
                rebuild.warnings.push(format!(
                    "Trigger {} uses column {old}. Apply the column rename on its own first, then make the other changes.",
                    trigger.name
                ));
            }
        }
    }

    let items = rebuild_items(&mut rebuild);
    // Draft keys that were meant to be in the text but are not.
    for (index, fk) in draft.foreign_keys.iter().enumerate() {
        if fk.original.is_some()
            && (fk.drop || fk_definition_changed(fk))
            && !rebuild.matched_fks.contains(&index)
        {
            rebuild.warnings.push(format!(
                "Foreign key {} is not in the CREATE TABLE text. Reload the table.",
                fk.name
            ));
        }
    }
    for (index, constraint) in draft.constraints.iter().enumerate() {
        if constraint.original.is_some()
            && (constraint.drop || constraint_definition_changed(constraint))
            && !rebuild.matched_constraints.contains(&index)
        {
            rebuild.warnings.push(format!(
                "Constraint {} is not in the CREATE TABLE text. Reload the table.",
                display_constraint(constraint)
            ));
        }
    }
    if !rebuild.warnings.is_empty() {
        return finish(S, Vec::new(), rebuild.warnings, false, false, None);
    }
    statements_for(rebuild, items)
}

fn display_constraint(constraint: &ConstraintDraft) -> String {
    if !constraint.name.trim().is_empty() {
        return constraint.name.clone();
    }
    match constraint.expression.as_deref() {
        Some(expression) => format!("CHECK ({expression})"),
        None => format!("UNIQUE ({})", constraint.columns.join(", ")),
    }
}

/// Column and constraint lines of the new table.
fn rebuild_items(rebuild: &mut Rebuild<'_, '_>) -> Vec<String> {
    let draft = rebuild.draft;
    let mut items = Vec::new();
    let parsed: Vec<Item> = rebuild.table.items.clone();
    for item in &parsed {
        match item {
            Item::Column(def) => {
                let column = rebuild.draft_column(&def.name);
                if column.is_some_and(|c| c.drop) {
                    continue;
                }
                let text = rebuild.text();
                let mut parts = Vec::new();
                parts.push(match rebuild.moves.new_name(&def.name) {
                    Some(to) => quote(S, to),
                    None => text.span(def.name_token, def.name_token).to_string(),
                });
                let typed = match column {
                    Some(c) if c.original.as_ref().is_some_and(|o| type_changed(o, c)) => {
                        c.data_type.trim().to_string()
                    }
                    _ => def.declared_type(text),
                };
                if !typed.is_empty() {
                    parts.push(typed);
                }
                let mut has_not_null = false;
                for constraint in &def.constraints {
                    if let Some(sql) = rebuild.constraint_text(constraint, column) {
                        has_not_null |= matches!(constraint.kind, ConstraintKind::NotNull);
                        parts.push(sql);
                    }
                }
                if let Some(c) = column {
                    if !c.nullable && !has_not_null {
                        parts.push("NOT NULL".into());
                    }
                    let default_moved = c
                        .original
                        .as_ref()
                        .is_some_and(|original| default_changed(original, c));
                    if default_moved {
                        if let Some(default) = c
                            .default_value
                            .as_deref()
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                        {
                            parts.push(format!("DEFAULT {default}"));
                        }
                    }
                    if rebuild
                        .inline_pk
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(c.name.trim()))
                    {
                        parts.push("PRIMARY KEY AUTOINCREMENT".into());
                    }
                }
                items.push(parts.join(" "));
            }
            Item::Constraint(constraint) => {
                if let Some(sql) = rebuild.constraint_text(constraint, None) {
                    items.push(sql);
                }
            }
        }
    }
    // New columns go after the existing ones, before the table constraints.
    let column_count = parsed
        .iter()
        .filter(|item| match item {
            Item::Column(def) => !rebuild.draft_column(&def.name).is_some_and(|c| c.drop),
            Item::Constraint(_) => false,
        })
        .count();
    let mut new_columns = Vec::new();
    for column in draft
        .columns
        .iter()
        .filter(|c| c.original.is_none() && !c.drop)
    {
        let mut line = column_definition(S, column);
        if rebuild
            .inline_pk
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(column.name.trim()))
        {
            line.push_str(" PRIMARY KEY AUTOINCREMENT");
        }
        new_columns.push(line);
    }
    let tail = items.split_off(column_count.min(items.len()));
    items.extend(new_columns);
    items.extend(tail);
    if rebuild.pk_rewrite && rebuild.inline_pk.is_none() {
        let mut pk: Vec<&ColumnDraft> = draft
            .columns
            .iter()
            .filter(|column| !column.drop && column.pk_position > 0)
            .collect();
        pk.sort_by_key(|column| column.pk_position);
        if !pk.is_empty() {
            let names: Vec<String> = pk.iter().map(|c| c.name.trim().to_string()).collect();
            items.push(format!("PRIMARY KEY ({})", quote_list(S, &names)));
        }
    }
    for constraint in &draft.constraints {
        let add = !constraint.drop
            && (constraint.original.is_none()
                || constraint_definition_changed(constraint)
                || constraint
                    .original
                    .as_ref()
                    .is_some_and(|o| o.name.trim() != constraint.name.trim()));
        if add {
            items.extend(constraint_body(S, constraint));
        }
    }
    for fk in &draft.foreign_keys {
        if !fk.drop && (fk.original.is_none() || fk_definition_changed(fk)) {
            items.push(foreign_key_body(S, fk));
        }
    }
    items
}

fn sql_string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The table's `INTEGER PRIMARY KEY` column, which is the rowid.
fn old_rowid_alias(table: &CreateTable<'_>) -> bool {
    let text = &table.text;
    let integer = |name: &str| {
        table.columns().any(|def| {
            def.name.eq_ignore_ascii_case(name)
                && def.declared_type(text).eq_ignore_ascii_case("INTEGER")
        })
    };
    for item in &table.items {
        match item {
            Item::Column(def) => {
                let pk = def
                    .constraints
                    .iter()
                    .any(|c| matches!(c.kind, ConstraintKind::PrimaryKey { .. }));
                if pk && def.declared_type(text).eq_ignore_ascii_case("INTEGER") {
                    return true;
                }
            }
            Item::Constraint(Constraint {
                kind: ConstraintKind::PrimaryKey { columns, .. },
                ..
            }) => {
                if columns.len() == 1 && integer(&columns[0]) {
                    return true;
                }
            }
            Item::Constraint(_) => {}
        }
    }
    false
}

fn statements_for(mut rebuild: Rebuild<'_, '_>, items: Vec<String>) -> DdlPlan {
    let draft = rebuild.draft;
    let name = draft.name.trim();
    let table_sql = quote(S, name);
    let temp_name = format!("mystique_rebuild_{name}");
    let temp_sql = quote(S, &temp_name);
    let mut statements = vec![
        steps::FOREIGN_KEYS_OFF.to_string(),
        steps::BEGIN.to_string(),
    ];

    for (from, to) in &rebuild.moves.renames {
        statements.push(format!(
            "ALTER TABLE {table_sql} RENAME COLUMN {} TO {}",
            quote(S, from),
            quote(S, to)
        ));
    }

    let body = if rebuild.table.multiline() {
        format!("(\n  {}\n)", items.join(",\n  "))
    } else {
        format!("({})", items.join(", "))
    };
    statements.push(format!(
        "CREATE TABLE {temp_sql} {body}{}",
        rebuild.table.options()
    ));

    // Copy every kept, stored column. The rowid too, unless a column is it.
    let mut copy = Vec::new();
    for def in rebuild.table.columns() {
        if rebuild.moves.is_dropped(&def.name) || def.is_generated() {
            continue;
        }
        let current = rebuild.moves.new_name(&def.name).unwrap_or(&def.name);
        copy.push(quote(S, current));
    }
    let new_pk: Vec<&ColumnDraft> = draft
        .columns
        .iter()
        .filter(|c| !c.drop && c.pk_position > 0)
        .collect();
    let new_alias = if rebuild.pk_rewrite {
        new_pk.len() == 1 && new_pk[0].data_type.trim().eq_ignore_ascii_case("INTEGER")
    } else {
        old_rowid_alias(&rebuild.table)
    };
    let reserved = draft.columns.iter().any(|c| {
        ["rowid", "oid", "_rowid_"]
            .iter()
            .any(|word| c.name.trim().eq_ignore_ascii_case(word))
    });
    let keep_rowid = !rebuild.table.without_rowid()
        && !old_rowid_alias(&rebuild.table)
        && !new_alias
        && !reserved;
    if keep_rowid {
        copy.insert(0, "rowid".into());
    }
    if copy.is_empty() {
        rebuild
            .notes
            .push("No column is kept, so the rows are not copied.".into());
    } else {
        let list = copy.join(", ");
        statements.push(format!(
            "INSERT INTO {temp_sql} ({list}) SELECT {list} FROM {table_sql}"
        ));
    }
    let autoincrement_kept = rebuild.table.text.tokens.iter().any(|token| {
        token.kind == Tok::Ident
            && rebuild.table.text.sql[token.start..token.end].eq_ignore_ascii_case("AUTOINCREMENT")
    }) && items
        .iter()
        .any(|item| item.to_ascii_uppercase().contains("AUTOINCREMENT"));
    if autoincrement_kept {
        // Keep the AUTOINCREMENT high-water mark, not just the largest row.
        statements.push(format!(
            "DELETE FROM sqlite_sequence WHERE name = {}",
            sql_string(&temp_name)
        ));
        statements.push(format!(
            "INSERT INTO sqlite_sequence (name, seq) SELECT {}, seq FROM sqlite_sequence WHERE name = {}",
            sql_string(&temp_name),
            sql_string(name)
        ));
    }
    statements.push(format!("DROP TABLE {table_sql}"));
    statements.push(steps::LEGACY_ALTER_ON.into());
    statements.push(format!("ALTER TABLE {temp_sql} RENAME TO {table_sql}"));
    statements.push(steps::LEGACY_ALTER_OFF.into());

    let mut indexes = 0usize;
    for index in draft
        .indexes
        .iter()
        .filter(|index| !index.primary && !index.drop)
    {
        let replay = index
            .original
            .as_ref()
            .filter(|_| !index_definition_changed(index))
            .and_then(|original| original.definition.as_deref());
        let sql = match (replay, &index.original) {
            (Some(definition), _) => match replay_index(&rebuild.moves, definition) {
                Ok(sql) => sql,
                Err(dropped) => {
                    rebuild.notes.push(format!(
                        "Index {} uses dropped column {dropped} and is removed.",
                        index.name
                    ));
                    continue;
                }
            },
            (None, Some(_)) if !index_definition_changed(index) => {
                // No stored text: rebuild it from its (renamed) columns.
                if let Some(dropped) = index.columns.iter().find(|c| rebuild.moves.is_dropped(c)) {
                    rebuild.notes.push(format!(
                        "Index {} uses dropped column {dropped} and is removed.",
                        index.name
                    ));
                    continue;
                }
                let mut renamed = index.clone();
                renamed.columns = index
                    .columns
                    .iter()
                    .map(|c| rebuild.moves.new_name(c).unwrap_or(c).to_string())
                    .collect();
                create_index_sql(S, draft, &renamed)
            }
            _ => create_index_sql(S, draft, index),
        };
        statements.push(sql);
        indexes += 1;
    }
    let mut triggers = 0usize;
    for trigger in &draft.triggers {
        let dropped = Text::new(&trigger.sql).ok().and_then(|text| {
            let last = text.len().checked_sub(1)?;
            rebuild.moves.dropped_in(&text, 0, last)
        });
        if let Some(dropped) = dropped {
            rebuild.notes.push(format!(
                "Trigger {} uses dropped column {dropped} and is removed.",
                trigger.name
            ));
            continue;
        }
        statements.push(trigger.sql.trim().to_string());
        triggers += 1;
    }
    if let Some(rename) = rename_sql(S, draft) {
        statements.push(rename);
    }
    statements.push(steps::FOREIGN_KEY_CHECK.into());
    statements.push(steps::COMMIT.into());
    statements.push(steps::FOREIGN_KEYS_ON.into());

    let mut note = format!(
        "SQLite rebuilds {name}: a new table from its CREATE TABLE, the rows copied, then {indexes} index(es) and {triggers} trigger(s) recreated. Foreign keys are checked before commit."
    );
    for extra in &rebuild.notes {
        note.push(' ');
        note.push_str(extra);
    }
    finish(S, statements, Vec::new(), true, true, Some(note))
}

/// An index's own SQL with renamed columns. `Err` names a dropped column it uses.
fn replay_index(moves: &ColumnMoves, definition: &str) -> Result<String, String> {
    let Ok(text) = Text::new(definition) else {
        return Ok(definition.trim().to_string());
    };
    // Columns come after `ON table`.
    let Some(on) = (0..text.len()).find(|&at| text.is_kw(at, "ON")) else {
        return Ok(definition.trim().to_string());
    };
    let first = on + 2;
    let last = text.len().saturating_sub(1);
    if first > last {
        return Ok(definition.trim().to_string());
    }
    if let Some(dropped) = moves.dropped_in(&text, first, last) {
        return Err(dropped);
    }
    let refs: Vec<usize> = text
        .column_refs(first, last)
        .into_iter()
        .filter(|(_, name)| moves.new_name(name).is_some())
        .map(|(at, _)| at)
        .collect();
    Ok(text
        .render(0, last, &|at| {
            if refs.contains(&at) {
                text.ident(at)
                    .and_then(|name| moves.new_name(&name).map(|to| quote(S, to)))
            } else {
                None
            }
        })
        .trim()
        .to_string())
}
