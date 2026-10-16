//! Structure edits and same-engine schema diff.
//!
//! The editor sends a draft. This module is the only place that turns that
//! draft into SQL or Mongo commands. A warning means the plan cannot be applied.

use std::collections::{HashMap, HashSet, VecDeque};

use mystique_sql::{quote_ident, SqlDialect};
use serde::{Deserialize, Serialize};

use crate::WriteRisk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlDialect {
    Postgres,
    Mysql,
    Sqlite,
    Mongo,
}

impl DdlDialect {
    pub fn from_engine(engine: &str, sql_dialect: &str) -> Self {
        let engine = engine.trim().to_ascii_lowercase();
        if engine == "mongodb" || engine == "documentdb" {
            return Self::Mongo;
        }
        match sql_dialect.trim().to_ascii_lowercase().as_str() {
            "mysql" => Self::Mysql,
            "sqlite" => Self::Sqlite,
            "postgresql" | "postgres" => Self::Postgres,
            _ => match engine.as_str() {
                "mysql" | "mariadb" | "tidb" => Self::Mysql,
                "sqlite" => Self::Sqlite,
                _ => Self::Postgres,
            },
        }
    }

    fn sql(self) -> SqlDialect {
        match self {
            Self::Mysql => SqlDialect::Mysql,
            Self::Sqlite => SqlDialect::Sqlite,
            Self::Postgres | Self::Mongo => SqlDialect::Postgres,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnOriginal {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub pk_position: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDraft {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub pk_position: u32,
    #[serde(default)]
    pub original: Option<ColumnOriginal>,
    #[serde(default)]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexOriginal {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexDraft {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
    /// Postgres `CREATE INDEX CONCURRENTLY`. Cannot share a transaction.
    #[serde(default)]
    pub concurrent: bool,
    #[serde(default)]
    pub original: Option<IndexOriginal>,
    #[serde(default)]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyOriginal {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyDraft {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(default)]
    pub original: Option<ForeignKeyOriginal>,
    #[serde(default)]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableDefinition {
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default = "default_table_kind")]
    pub kind: String,
    #[serde(default)]
    pub columns: Vec<ColumnDraft>,
    #[serde(default)]
    pub indexes: Vec<IndexDraft>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyDraft>,
    /// SQLite `sqlite_master.sql` captured when the definition was read.
    #[serde(default)]
    pub schema_revision: Option<String>,
    #[serde(default)]
    pub validator: Option<String>,
    #[serde(default)]
    pub original_validator: Option<String>,
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub drop_table: bool,
    /// After an explicit rename pair, the table should end with this name.
    #[serde(default)]
    pub rename_to: Option<String>,
}

fn default_table_kind() -> String {
    "table".into()
}

impl ColumnDraft {
    /// A column just read from the catalog. `original` matches the live row.
    pub fn loaded(
        name: impl Into<String>,
        data_type: impl Into<String>,
        nullable: bool,
        pk_position: u32,
    ) -> Self {
        let name = name.into();
        let data_type = data_type.into();
        Self {
            name: name.clone(),
            data_type: data_type.clone(),
            nullable,
            pk_position,
            original: Some(ColumnOriginal {
                name,
                data_type,
                nullable,
                pk_position,
            }),
            drop: false,
        }
    }
}

impl IndexDraft {
    pub fn loaded(
        name: impl Into<String>,
        columns: Vec<String>,
        unique: bool,
        primary: bool,
    ) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            columns: columns.clone(),
            unique,
            primary,
            concurrent: false,
            original: Some(IndexOriginal {
                name,
                columns,
                unique,
                primary,
            }),
            drop: false,
        }
    }
}

impl ForeignKeyDraft {
    pub fn loaded(
        name: impl Into<String>,
        columns: Vec<String>,
        ref_schema: Option<String>,
        ref_table: impl Into<String>,
        ref_columns: Vec<String>,
    ) -> Self {
        let name = name.into();
        let ref_table = ref_table.into();
        Self {
            name: name.clone(),
            columns: columns.clone(),
            ref_schema: ref_schema.clone(),
            ref_table: ref_table.clone(),
            ref_columns: ref_columns.clone(),
            original: Some(ForeignKeyOriginal {
                name,
                columns,
                ref_schema,
                ref_table,
                ref_columns,
            }),
            drop: false,
        }
    }
}

impl TableDefinition {
    pub fn loaded(
        database: Option<String>,
        schema: Option<String>,
        name: impl Into<String>,
        kind: &str,
    ) -> Self {
        Self {
            database,
            schema,
            name: name.into(),
            kind: kind.into(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            schema_revision: None,
            validator: None,
            original_validator: None,
            create: false,
            drop_table: false,
            rename_to: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DdlPlan {
    pub statements: Vec<String>,
    pub warnings: Vec<String>,
    pub transactional: bool,
    pub requires_rebuild: bool,
    /// Shown with the preview. Not a warning, so it does not block apply.
    #[serde(default)]
    pub note: Option<String>,
}

impl DdlPlan {
    pub fn applyable(&self) -> bool {
        self.warnings.is_empty() && !self.statements.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaChange {
    pub name: String,
    pub action: String,
    pub plan: DdlPlan,
    pub draft: TableDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMigration {
    pub changes: Vec<SchemaChange>,
    pub warnings: Vec<String>,
}

pub fn plan_table_alter(dialect: DdlDialect, draft: &TableDefinition) -> DdlPlan {
    let mut warnings = Vec::new();
    if draft.create && draft.drop_table {
        warnings.push("A table cannot be created and dropped in one change.".into());
    }
    if draft.name.trim().is_empty() {
        warnings.push("Table name is empty.".into());
    }
    if !draft.drop_table
        && !draft.create
        && draft.kind.eq_ignore_ascii_case("view")
        && has_structural_edit(draft)
    {
        warnings.push("Views cannot be edited here.".into());
    }
    validate_columns(dialect, draft, &mut warnings);
    validate_indexes(draft, &mut warnings);
    validate_foreign_keys(draft, &mut warnings);

    if dialect == DdlDialect::Mongo {
        return plan_mongo(draft, warnings);
    }
    if draft.drop_table {
        return finish(
            dialect,
            vec![format!("DROP TABLE {}", qualify(dialect, draft))],
            warnings,
            false,
            false,
            None,
        );
    }
    if draft.create {
        return plan_create(dialect, draft, warnings);
    }
    if !warnings.is_empty() {
        return finish(dialect, Vec::new(), warnings, false, false, None);
    }
    match dialect {
        DdlDialect::Sqlite => plan_sqlite(draft),
        DdlDialect::Postgres => plan_sql(dialect, draft, true),
        DdlDialect::Mysql => plan_sql(dialect, draft, false),
        DdlDialect::Mongo => unreachable!(),
    }
}

pub fn ddl_risk(draft: &TableDefinition) -> WriteRisk {
    if draft.drop_table || draft.columns.iter().any(|column| column.drop) {
        WriteRisk::Destructive
    } else {
        WriteRisk::Write
    }
}

pub fn migration_risk(changes: &[SchemaChange]) -> WriteRisk {
    if changes.iter().any(|change| {
        change.action == "drop"
            || change.plan.statements.iter().any(|sql| {
                sql.to_ascii_uppercase().contains("DROP COLUMN") || sql.starts_with("DROP TABLE")
            })
    }) {
        WriteRisk::Destructive
    } else {
        WriteRisk::Write
    }
}

/// Make `target` match `source`. Renames are only the pairs the user supplied.
pub fn plan_schema_diff(
    dialect: DdlDialect,
    source: &[TableDefinition],
    target: &[TableDefinition],
    renames: &[RenamePair],
) -> SchemaMigration {
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

fn plan_create(dialect: DdlDialect, draft: &TableDefinition, mut warnings: Vec<String>) -> DdlPlan {
    if dialect == DdlDialect::Mongo {
        warnings.push("Creating a Mongo collection from a column draft is not supported.".into());
        return finish(dialect, Vec::new(), warnings, false, false, None);
    }
    let mut statements = vec![create_table_sql(dialect, draft)];
    statements.extend(index_statements(dialect, draft, false));
    statements.extend(foreign_key_statements(dialect, draft));
    let transactional = dialect == DdlDialect::Postgres;
    finish(dialect, statements, warnings, transactional, false, None)
}

fn plan_sql(dialect: DdlDialect, draft: &TableDefinition, postgres: bool) -> DdlPlan {
    let mut warnings = Vec::new();
    if pk_changed(draft) {
        warnings.push("Changing the primary key is not supported on this engine.".into());
    }
    let mut statements = Vec::new();
    let table = qualify(dialect, draft);
    for column in &draft.columns {
        if column.drop {
            if column.original.is_some() {
                statements.push(format!(
                    "ALTER TABLE {table} DROP COLUMN {}",
                    quote(dialect, &column.name)
                ));
            }
            continue;
        }
        match &column.original {
            None => {
                if !column.nullable {
                    warnings.push(format!(
                        "Add column {} as nullable. NOT NULL without a default fails when the table already has rows.",
                        column.name
                    ));
                }
                statements.push(format!(
                    "ALTER TABLE {table} ADD COLUMN {} {}",
                    quote(dialect, &column.name),
                    column.data_type.trim()
                ));
            }
            Some(original) => {
                if !original.name.eq_ignore_ascii_case(&column.name) {
                    statements.push(format!(
                        "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                        quote(dialect, &original.name),
                        quote(dialect, &column.name)
                    ));
                }
                if !original
                    .data_type
                    .eq_ignore_ascii_case(column.data_type.trim())
                    || original.nullable != column.nullable
                {
                    if postgres {
                        if !original
                            .data_type
                            .eq_ignore_ascii_case(column.data_type.trim())
                        {
                            statements.push(format!(
                                "ALTER TABLE {table} ALTER COLUMN {} TYPE {}",
                                quote(dialect, &column.name),
                                column.data_type.trim()
                            ));
                        }
                        if original.nullable != column.nullable {
                            let verb = if column.nullable { "DROP" } else { "SET" };
                            statements.push(format!(
                                "ALTER TABLE {table} ALTER COLUMN {} {verb} NOT NULL",
                                quote(dialect, &column.name)
                            ));
                        }
                    } else {
                        let null_sql = if column.nullable { "NULL" } else { "NOT NULL" };
                        statements.push(format!(
                            "ALTER TABLE {table} MODIFY COLUMN {} {} {null_sql}",
                            quote(dialect, &column.name),
                            column.data_type.trim()
                        ));
                    }
                }
            }
        }
    }
    let concurrent = postgres
        && draft
            .indexes
            .iter()
            .any(|index| index.concurrent && !index.drop);
    if concurrent && (statements.len() + index_change_count(draft) > 1) {
        warnings.push(
            "CREATE INDEX CONCURRENTLY cannot share a transaction with other changes.".into(),
        );
    }
    statements.extend(index_statements(dialect, draft, true));
    statements.extend(foreign_key_statements(dialect, draft));
    if let Some(rename) = rename_sql(dialect, draft) {
        statements.push(rename);
    }
    let note = if postgres {
        None
    } else {
        Some(
            "MySQL applies each statement immediately. A later failure leaves the earlier ones in place."
                .into(),
        )
    };
    finish(
        dialect,
        statements,
        warnings,
        postgres && !concurrent,
        false,
        note,
    )
}

fn plan_sqlite(draft: &TableDefinition) -> DdlPlan {
    let mut warnings = Vec::new();
    if pk_changed(draft) || needs_sqlite_rebuild(draft) {
        if draft
            .schema_revision
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            warnings
                .push("Reload the table before rebuilding. The schema revision is missing.".into());
        }
        let mut statements = vec!["PRAGMA foreign_keys=OFF".into(), "BEGIN".into()];
        statements.push(create_table_sql(
            DdlDialect::Sqlite,
            &renamed_rebuild(draft),
        ));
        statements.push(sqlite_copy_sql(draft));
        statements.push(format!("DROP TABLE {}", qualify(DdlDialect::Sqlite, draft)));
        statements.push(format!(
            "ALTER TABLE {} RENAME TO {}",
            quote(DdlDialect::Sqlite, &rebuild_name(&draft.name)),
            quote(DdlDialect::Sqlite, &draft.name)
        ));
        statements.extend(index_statements(DdlDialect::Sqlite, draft, false));
        statements.push("COMMIT".into());
        statements.push("PRAGMA foreign_keys=ON".into());
        if let Some(rename) = rename_sql(DdlDialect::Sqlite, draft) {
            statements.push(rename);
        }
        return finish(
            DdlDialect::Sqlite,
            statements,
            warnings,
            true,
            true,
            Some(
                "SQLite rebuilds the table in one transaction, then checks the schema revision."
                    .into(),
            ),
        );
    }
    let mut statements = Vec::new();
    let table = qualify(DdlDialect::Sqlite, draft);
    for column in &draft.columns {
        if column.original.is_none() && !column.drop {
            if !column.nullable {
                warnings.push(format!(
                    "Add column {} as nullable. SQLite cannot add a NOT NULL column without a default.",
                    column.name
                ));
            }
            statements.push(format!(
                "ALTER TABLE {table} ADD COLUMN {} {}",
                quote(DdlDialect::Sqlite, &column.name),
                column.data_type.trim()
            ));
        }
    }
    statements.extend(index_statements(DdlDialect::Sqlite, draft, true));
    if let Some(rename) = rename_sql(DdlDialect::Sqlite, draft) {
        statements.push(rename);
    }
    finish(DdlDialect::Sqlite, statements, warnings, false, false, None)
}

fn plan_mongo(draft: &TableDefinition, mut warnings: Vec<String>) -> DdlPlan {
    if draft.columns.iter().any(|column| column_changes(column)) {
        warnings.push("Mongo collections have no columns. Edit indexes or the validator.".into());
    }
    if draft.foreign_keys.iter().any(|fk| fk_changes(fk)) {
        warnings.push("Mongo collections have no foreign keys.".into());
    }
    let mut statements = Vec::new();
    if draft.drop_table {
        statements.push(mongo_command(&serde_json::json!({ "drop": draft.name })));
    }
    for index in &draft.indexes {
        if index.drop && index.original.is_some() {
            statements.push(mongo_command(&serde_json::json!({
                "dropIndexes": draft.name,
                "index": index.name,
            })));
            continue;
        }
        if index.original.is_none() && !index.drop {
            let mut key = serde_json::Map::new();
            for column in &index.columns {
                key.insert(column.clone(), serde_json::json!(1));
            }
            statements.push(mongo_command(&serde_json::json!({
                "createIndexes": draft.name,
                "indexes": [{
                    "key": key,
                    "name": index.name,
                    "unique": index.unique,
                }],
            })));
        }
    }
    if draft.validator != draft.original_validator {
        match draft
            .validator
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            None => warnings.push("Clearing a Mongo validator is not supported here.".into()),
            Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
                Ok(validator) => statements.push(mongo_command(&serde_json::json!({
                    "collMod": draft.name,
                    "validator": validator,
                }))),
                Err(_) => warnings.push("Mongo validator must be a JSON object.".into()),
            },
        }
    }
    finish(DdlDialect::Mongo, statements, warnings, false, false, None)
}

fn finish(
    dialect: DdlDialect,
    statements: Vec<String>,
    warnings: Vec<String>,
    transactional: bool,
    requires_rebuild: bool,
    note: Option<String>,
) -> DdlPlan {
    let _ = dialect;
    let blocked = !warnings.is_empty();
    DdlPlan {
        statements: if blocked { Vec::new() } else { statements },
        transactional: transactional && !blocked,
        requires_rebuild: requires_rebuild && !blocked,
        warnings,
        note,
    }
}

fn validate_columns(dialect: DdlDialect, draft: &TableDefinition, warnings: &mut Vec<String>) {
    let mut seen = HashSet::new();
    let mut live = 0usize;
    // Unchanged types are not spliced again. A rebuild or CREATE reprints every type.
    let reprint_all = draft.create
        || (dialect == DdlDialect::Sqlite && (pk_changed(draft) || needs_sqlite_rebuild(draft)));
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

fn validate_indexes(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
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

fn validate_foreign_keys(draft: &TableDefinition, warnings: &mut Vec<String>) {
    let columns = live_column_names(draft);
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
        for column in &fk.columns {
            if !columns.iter().any(|name| name.eq_ignore_ascii_case(column)) {
                warnings.push(format!(
                    "Foreign key {} uses {}, which is not a column on this table.",
                    fk.name, column
                ));
            }
        }
    }
}

fn plan_notes_sql(dialect: DdlDialect) -> SqlDialect {
    dialect.sql()
}

fn quote(dialect: DdlDialect, name: &str) -> String {
    quote_ident(plan_notes_sql(dialect), name.trim())
}

fn qualify(dialect: DdlDialect, draft: &TableDefinition) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if dialect == DdlDialect::Mysql {
        if let Some(database) = draft
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(database);
        }
    }
    if dialect != DdlDialect::Sqlite {
        if let Some(schema) = draft
            .schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !schema.eq_ignore_ascii_case("main") {
                parts.push(schema);
            }
        }
    }
    parts.push(draft.name.trim());
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(|part| quote(dialect, part))
        .collect::<Vec<_>>()
        .join(".")
}

fn create_table_sql(dialect: DdlDialect, draft: &TableDefinition) -> String {
    let mut lines = Vec::new();
    for column in draft.columns.iter().filter(|column| !column.drop) {
        let null_sql = if column.nullable { "NULL" } else { "NOT NULL" };
        lines.push(format!(
            "{} {} {null_sql}",
            quote(dialect, &column.name),
            column.data_type.trim()
        ));
    }
    let mut pk: Vec<&ColumnDraft> = draft
        .columns
        .iter()
        .filter(|column| !column.drop && column.pk_position > 0)
        .collect();
    pk.sort_by_key(|column| column.pk_position);
    if !pk.is_empty() {
        let cols = pk
            .iter()
            .map(|column| quote(dialect, &column.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("PRIMARY KEY ({cols})"));
    }
    format!(
        "CREATE TABLE {} ({})",
        qualify(dialect, draft),
        lines.join(", ")
    )
}

fn drop_index_sql(dialect: DdlDialect, draft: &TableDefinition, name: &str) -> String {
    let quoted = quote(dialect, name);
    match dialect {
        DdlDialect::Mysql => {
            format!(
                "ALTER TABLE {} DROP INDEX {quoted}",
                qualify(dialect, draft)
            )
        }
        DdlDialect::Postgres => {
            let index = match draft
                .schema
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(schema) => format!("{}.{}", quote(dialect, schema), quoted),
                None => quoted,
            };
            format!("DROP INDEX {index}")
        }
        _ => format!("DROP INDEX {quoted}"),
    }
}

fn index_statements(
    dialect: DdlDialect,
    draft: &TableDefinition,
    include_drops: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let table = qualify(dialect, draft);
    for index in &draft.indexes {
        if index.primary {
            continue;
        }
        if include_drops && index.drop && index.original.is_some() {
            out.push(drop_index_sql(dialect, draft, &index.name));
            continue;
        }
        let changed = index.original.as_ref().is_some_and(|original| {
            original.unique != index.unique || original.columns != index.columns
        });
        if changed && include_drops {
            out.push(drop_index_sql(dialect, draft, &index.name));
        }
        if (index.original.is_none() || changed) && !index.drop {
            let unique = if index.unique { "UNIQUE " } else { "" };
            let concurrently = if index.concurrent && dialect == DdlDialect::Postgres {
                "CONCURRENTLY "
            } else {
                ""
            };
            let cols = index
                .columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(format!(
                "CREATE {unique}INDEX {concurrently}{} ON {table} ({cols})",
                quote(dialect, &index.name)
            ));
        }
    }
    out
}

fn foreign_key_statements(dialect: DdlDialect, draft: &TableDefinition) -> Vec<String> {
    let mut out = Vec::new();
    let table = qualify(dialect, draft);
    for fk in &draft.foreign_keys {
        if fk.drop && fk.original.is_some() {
            let sql = if dialect == DdlDialect::Mysql {
                format!(
                    "ALTER TABLE {table} DROP FOREIGN KEY {}",
                    quote(dialect, &fk.name)
                )
            } else {
                format!(
                    "ALTER TABLE {table} DROP CONSTRAINT {}",
                    quote(dialect, &fk.name)
                )
            };
            out.push(sql);
            continue;
        }
        if fk.original.is_none() && !fk.drop {
            let cols = fk
                .columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            let refs = fk
                .ref_columns
                .iter()
                .map(|column| quote(dialect, column))
                .collect::<Vec<_>>()
                .join(", ");
            let ref_table = dialect.sql().quote_path(
                [
                    fk.ref_schema.as_deref().unwrap_or(""),
                    fk.ref_table.as_str(),
                ]
                .into_iter()
                .filter(|part| !part.is_empty()),
            );
            out.push(format!(
                "ALTER TABLE {table} ADD CONSTRAINT {} FOREIGN KEY ({cols}) REFERENCES {ref_table} ({refs})",
                quote(dialect, &fk.name)
            ));
        }
    }
    out
}

fn rename_sql(dialect: DdlDialect, draft: &TableDefinition) -> Option<String> {
    let next = draft
        .rename_to
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    if next == draft.name.trim() {
        return None;
    }
    Some(format!(
        "ALTER TABLE {} RENAME TO {}",
        qualify(dialect, draft),
        quote(dialect, next)
    ))
}

fn sqlite_copy_sql(draft: &TableDefinition) -> String {
    let mut into_cols = Vec::new();
    let mut from_cols = Vec::new();
    for column in draft.columns.iter().filter(|column| !column.drop) {
        let Some(original) = &column.original else {
            continue;
        };
        into_cols.push(quote(DdlDialect::Sqlite, &column.name));
        from_cols.push(quote(DdlDialect::Sqlite, &original.name));
    }
    format!(
        "INSERT INTO {} ({}) SELECT {} FROM {}",
        quote(DdlDialect::Sqlite, &rebuild_name(&draft.name)),
        into_cols.join(", "),
        from_cols.join(", "),
        qualify(DdlDialect::Sqlite, draft)
    )
}

fn renamed_rebuild(draft: &TableDefinition) -> TableDefinition {
    let mut copy = draft.clone();
    copy.name = rebuild_name(&draft.name);
    copy.schema = None;
    copy.database = None;
    copy
}

fn rebuild_name(name: &str) -> String {
    format!("{name}__mystique_new")
}

fn needs_sqlite_rebuild(draft: &TableDefinition) -> bool {
    draft.columns.iter().any(|column| {
        column.drop && column.original.is_some()
            || column.original.as_ref().is_some_and(|original| {
                !original.name.eq_ignore_ascii_case(&column.name)
                    || !original
                        .data_type
                        .eq_ignore_ascii_case(column.data_type.trim())
                    || original.nullable != column.nullable
                    || original.pk_position != column.pk_position
            })
    }) || draft.foreign_keys.iter().any(fk_changes)
}

fn pk_changed(draft: &TableDefinition) -> bool {
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

fn column_changes(column: &ColumnDraft) -> bool {
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

fn fk_changes(fk: &ForeignKeyDraft) -> bool {
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

fn has_structural_edit(draft: &TableDefinition) -> bool {
    draft.columns.iter().any(column_changes)
        || draft.indexes.iter().any(|index| {
            index.drop && index.original.is_some() || index.original.is_none() && !index.drop
        })
        || draft.foreign_keys.iter().any(fk_changes)
}

fn index_change_count(draft: &TableDefinition) -> usize {
    draft
        .indexes
        .iter()
        .filter(|index| {
            !index.primary && (index.drop || index.original.is_none() || index.concurrent)
        })
        .count()
}

fn live_column_names(draft: &TableDefinition) -> Vec<String> {
    draft
        .columns
        .iter()
        .filter(|column| !column.drop)
        .map(|column| column.name.clone())
        .collect()
}

fn type_is_safe(data_type: &str) -> bool {
    let text = data_type.trim();
    !text.is_empty()
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '(' | ')' | ',' | '_' | '[' | ']' | '.')
        })
}

fn mongo_command(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

fn align(desired: &TableDefinition, current: &TableDefinition) -> TableDefinition {
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

fn column_original(column: &ColumnDraft) -> ColumnOriginal {
    ColumnOriginal {
        name: column.name.clone(),
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        pk_position: column.pk_position,
    }
}

fn index_original(index: &IndexDraft) -> IndexOriginal {
    IndexOriginal {
        name: index.name.clone(),
        columns: index.columns.clone(),
        unique: index.unique,
        primary: index.primary,
    }
}

fn fk_original(fk: &ForeignKeyDraft) -> ForeignKeyOriginal {
    ForeignKeyOriginal {
        name: fk.name.clone(),
        columns: fk.columns.clone(),
        ref_schema: fk.ref_schema.clone(),
        ref_table: fk.ref_table.clone(),
        ref_columns: fk.ref_columns.clone(),
    }
}

fn order_changes(
    changes: &mut Vec<SchemaChange>,
    drafts: &[TableDefinition],
) -> Result<(), String> {
    let creates: HashSet<String> = drafts
        .iter()
        .filter(|draft| draft.create)
        .map(|draft| draft.name.clone())
        .collect();
    let mut edges = Vec::new();
    for draft in drafts.iter().filter(|draft| draft.create) {
        for fk in &draft.foreign_keys {
            if creates.contains(&fk.ref_table) {
                edges.push((fk.ref_table.clone(), draft.name.clone()));
            }
        }
    }
    let create_order = topo(&creates.into_iter().collect::<Vec<_>>(), &edges)?;
    changes.sort_by_key(|change| {
        let group = match change.action.as_str() {
            "create" => 0,
            "alter" => 1,
            "unchanged" => 2,
            _ => 3,
        };
        let rank = create_order
            .get(&change.name)
            .copied()
            .unwrap_or(usize::MAX);
        (group, rank, change.name.clone())
    });
    Ok(())
}

fn topo(nodes: &[String], edges: &[(String, String)]) -> Result<HashMap<String, usize>, String> {
    let mut incoming: HashMap<String, usize> = nodes.iter().map(|name| (name.clone(), 0)).collect();
    let mut next: HashMap<String, Vec<String>> = HashMap::new();
    for (before, after) in edges {
        *incoming.entry(after.clone()).or_insert(0) += 1;
        next.entry(before.clone()).or_default().push(after.clone());
    }
    let mut queue: VecDeque<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(name, _)| name.clone())
        .collect();
    let mut order = HashMap::new();
    let mut rank = 0usize;
    while let Some(name) = queue.pop_front() {
        order.insert(name.clone(), rank);
        rank += 1;
        for child in next.get(&name).into_iter().flatten() {
            if let Some(count) = incoming.get_mut(child) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    queue.push_back(child.clone());
                }
            }
        }
    }
    if order.len() != nodes.len() {
        return Err("Foreign keys form a cycle. Apply these tables one at a time.".into());
    }
    Ok(order)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDdlResult {
    pub applied: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeTableDefinitionRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTableDefinitionsRequest {
    pub session_id: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDdlRequest {
    pub session_id: String,
    pub draft: TableDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDdlRequest {
    pub session_id: String,
    pub draft: TableDefinition,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSchemaDiffRequest {
    pub session_id: String,
    pub source: Vec<TableDefinition>,
    pub target: Vec<TableDefinition>,
    #[serde(default)]
    pub renames: Vec<RenamePair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySchemaDiffRequest {
    pub session_id: String,
    pub source: Vec<TableDefinition>,
    pub target: Vec<TableDefinition>,
    #[serde(default)]
    pub renames: Vec<RenamePair>,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_column(name: &str, nullable: bool) -> ColumnDraft {
        ColumnDraft {
            name: name.into(),
            data_type: "text".into(),
            nullable,
            pk_position: 0,
            original: None,
            drop: false,
        }
    }

    fn existing(name: &str) -> ColumnDraft {
        let mut column = text_column(name, true);
        column.original = Some(column_original(&column));
        column
    }

    fn table(name: &str, columns: Vec<ColumnDraft>) -> TableDefinition {
        TableDefinition {
            database: None,
            schema: Some("public".into()),
            name: name.into(),
            kind: "table".into(),
            columns,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            schema_revision: None,
            validator: None,
            original_validator: None,
            create: false,
            drop_table: false,
            rename_to: None,
        }
    }

    #[test]
    fn postgres_add_nullable_text_previews_the_alter() {
        let mut draft = table("orders", vec![existing("id"), text_column("note", true)]);
        draft.schema = Some("public".into());
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert_eq!(
            plan.statements,
            vec!["ALTER TABLE \"public\".\"orders\" ADD COLUMN \"note\" text"]
        );
    }

    #[test]
    fn not_null_without_a_default_cannot_be_applied() {
        let draft = table("orders", vec![existing("id"), text_column("note", false)]);
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(!plan.applyable());
        assert!(plan.statements.is_empty());
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("nullable")));
    }

    #[test]
    fn sqlite_add_is_alter_and_a_type_change_rebuilds() {
        let mut add = table("orders", vec![existing("id"), text_column("note", true)]);
        add.schema_revision = Some("create".into());
        let plan = plan_table_alter(DdlDialect::Sqlite, &add);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(!plan.requires_rebuild);
        assert_eq!(
            plan.statements,
            vec!["ALTER TABLE \"orders\" ADD COLUMN \"note\" text"]
        );

        let mut changed = existing("id");
        changed.data_type = "integer".into();
        let mut rebuild = table("orders", vec![changed]);
        rebuild.schema_revision = Some("CREATE TABLE orders (id text)".into());
        let plan = plan_table_alter(DdlDialect::Sqlite, &rebuild);
        assert!(plan.requires_rebuild, "{plan:?}");
        assert!(plan.statements.iter().any(|sql| sql.contains("BEGIN")));
        assert!(plan
            .statements
            .iter()
            .any(|sql| sql.contains("INSERT INTO")));
        assert!(plan
            .statements
            .iter()
            .any(|sql| sql.starts_with("PRAGMA foreign_keys=OFF")));
        assert!(plan
            .statements
            .last()
            .is_some_and(|sql| sql.starts_with("PRAGMA foreign_keys=ON")));
    }

    #[test]
    fn sqlite_rebuild_without_a_revision_is_refused() {
        let mut changed = existing("id");
        changed.data_type = "integer".into();
        let plan = plan_table_alter(DdlDialect::Sqlite, &table("orders", vec![changed]));
        assert!(!plan.applyable());
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("revision")));
    }

    #[test]
    fn concurrent_index_cannot_share_a_transaction() {
        let mut draft = table("orders", vec![existing("id"), text_column("note", true)]);
        draft.indexes.push(IndexDraft {
            name: "orders_note".into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
            concurrent: true,
            original: None,
            drop: false,
        });
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(!plan.applyable());
        let mut only = table("orders", vec![existing("note")]);
        only.indexes.push(IndexDraft {
            name: "orders_note".into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
            concurrent: true,
            original: None,
            drop: false,
        });
        let plan = plan_table_alter(DdlDialect::Postgres, &only);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(!plan.transactional);
        assert!(plan.statements[0].contains("CONCURRENTLY"));
    }

    #[test]
    fn mysql_alter_is_not_transactional() {
        let plan = plan_table_alter(
            DdlDialect::Mysql,
            &table("orders", vec![existing("id"), text_column("note", true)]),
        );
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(!plan.transactional);
        assert!(plan.statements[0].contains("ADD COLUMN"));
        assert!(plan.note.as_deref().unwrap_or("").contains("MySQL"));
    }

    #[test]
    fn mongo_columns_are_refused_and_indexes_are_commands() {
        let mut draft = table("orders", vec![text_column("note", true)]);
        draft.kind = "collection".into();
        let plan = plan_table_alter(DdlDialect::Mongo, &draft);
        assert!(!plan.applyable());
        let mut indexes = table("orders", vec![]);
        indexes.kind = "collection".into();
        indexes.indexes.push(IndexDraft {
            name: "note_1".into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
            concurrent: false,
            original: None,
            drop: false,
        });
        let plan = plan_table_alter(DdlDialect::Mongo, &indexes);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(plan.statements[0].contains("createIndexes"));
        assert!(plan.statements[0].contains("note"));
    }

    #[test]
    fn diff_does_not_guess_renames_and_orders_creates() {
        let source_orders = table("orders", vec![existing("id"), text_column("note", true)]);
        let mut source_child = table("lines", vec![existing("id"), existing("order_id")]);
        source_child.foreign_keys.push(ForeignKeyDraft {
            name: "lines_order".into(),
            columns: vec!["order_id".into(), "id".into()],
            ref_schema: None,
            ref_table: "orders".into(),
            ref_columns: vec!["id".into(), "note".into()],
            original: None,
            drop: false,
        });
        let target_orders = table("orders", vec![existing("id")]);
        let target_old = table("order_lines", vec![existing("id")]);
        let migration = plan_schema_diff(
            DdlDialect::Postgres,
            &[source_orders, source_child],
            &[target_orders, target_old],
            &[],
        );
        let actions: Vec<_> = migration
            .changes
            .iter()
            .map(|change| (change.name.as_str(), change.action.as_str()))
            .collect();
        assert!(
            actions.contains(&("order_lines", "drop")),
            "a different name is not a rename: {actions:?}"
        );
        assert!(actions.contains(&("lines", "create")));
        let orders = migration
            .changes
            .iter()
            .find(|change| change.name == "orders")
            .expect("orders");
        assert_eq!(orders.action, "alter");
        assert!(orders
            .plan
            .statements
            .iter()
            .any(|sql| sql.contains("ADD COLUMN")));
        let create_at = migration
            .changes
            .iter()
            .position(|change| change.name == "orders" && change.action == "create");
        let lines_at = migration
            .changes
            .iter()
            .position(|change| change.name == "lines")
            .expect("lines");
        if let Some(create_at) = create_at {
            assert!(create_at < lines_at);
        }
        let paired = plan_schema_diff(
            DdlDialect::Postgres,
            &[table("lines", vec![existing("id")])],
            &[table("order_lines", vec![existing("id")])],
            &[RenamePair {
                from: "lines".into(),
                to: "order_lines".into(),
            }],
        );
        assert!(paired.changes.iter().any(|change| {
            change.action == "alter"
                && change
                    .plan
                    .statements
                    .iter()
                    .any(|sql| sql.contains("RENAME TO"))
        }));
        assert!(!paired.changes.iter().any(|change| change.action == "drop"));
    }

    #[test]
    fn an_unchanged_enum_does_not_block_adding_a_column() {
        let mut status = existing("status");
        status.data_type = "enum('open','closed')".into();
        if let Some(original) = status.original.as_mut() {
            original.data_type = status.data_type.clone();
        }
        let draft = table("orders", vec![status, text_column("note", true)]);
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(plan.statements.iter().any(|sql| sql.contains("ADD COLUMN")));
    }

    #[test]
    fn a_new_column_cannot_smuggle_sql_in_the_type() {
        let mut note = text_column("note", true);
        note.data_type = "text; drop table orders".into();
        let draft = table("orders", vec![existing("id"), note]);
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(!plan.applyable());
        assert!(plan.statements.is_empty());
    }

    #[test]
    fn postgres_drops_an_index_in_its_schema() {
        let mut draft = table("orders", vec![existing("note")]);
        draft.indexes.push(IndexDraft {
            name: "orders_note".into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
            concurrent: false,
            original: Some(IndexOriginal {
                name: "orders_note".into(),
                columns: vec!["note".into()],
                unique: false,
                primary: false,
            }),
            drop: true,
        });
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert_eq!(
            plan.statements,
            vec!["DROP INDEX \"public\".\"orders_note\""]
        );
    }
}
