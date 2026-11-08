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
    /// MySQL: the column definition after its name, as `SHOW CREATE TABLE`
    /// prints it. MODIFY is rebuilt from it so DEFAULT, ON UPDATE,
    /// AUTO_INCREMENT, COMMENT, charset, and collation survive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
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
                definition: None,
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
    validate_columns(draft, &mut warnings);
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
                        match mysql_modify_definition(original, column) {
                            Ok(definition) => statements.push(format!(
                                "ALTER TABLE {table} MODIFY COLUMN {} {definition}",
                                quote(dialect, &column.name),
                            )),
                            Err(warning) => warnings.push(warning),
                        }
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

/// The full MySQL column definition for MODIFY: the new type and NULL-ness,
/// with every other attribute copied from the original definition.
fn mysql_modify_definition(
    original: &ColumnOriginal,
    column: &ColumnDraft,
) -> Result<String, String> {
    let reload = || {
        format!(
            "Reload the table before changing column {}. MODIFY needs its full definition, or it drops DEFAULT, AUTO_INCREMENT, COMMENT, and charset.",
            original.name
        )
    };
    let definition = original.definition.as_deref().ok_or_else(reload)?.trim();
    let old_type = original.data_type.trim();
    let tail = definition
        .get(..old_type.len())
        .filter(|head| head.eq_ignore_ascii_case(old_type))
        .and_then(|_| definition.get(old_type.len()..))
        .filter(|rest| rest.is_empty() || rest.starts_with(' '))
        .ok_or_else(reload)?;
    let mut words = top_level_words(tail);
    let is = |words: &[String], at: usize, want: &str| {
        words
            .get(at)
            .is_some_and(|word| word.eq_ignore_ascii_case(want))
    };

    let new_type = column.data_type.trim();
    if !new_type.eq_ignore_ascii_case(old_type) && !is_text_type(new_type) {
        // CHARACTER SET / COLLATE only apply to text types.
        let mut at = 0;
        while at < words.len() {
            let span = if is(&words, at, "CHARACTER") && is(&words, at + 1, "SET") {
                3
            } else if is(&words, at, "CHARSET") || is(&words, at, "COLLATE") {
                2
            } else {
                at += 1;
                continue;
            };
            words.drain(at..(at + span).min(words.len()));
        }
    }

    // Take out the old NULL / NOT NULL; remember where it sat.
    let mut slot = None;
    let mut at = 0;
    while at < words.len() {
        if is(&words, at, "NOT") && is(&words, at + 1, "NULL") {
            words.drain(at..at + 2);
            slot.get_or_insert(at);
        } else if is(&words, at, "NULL") && !(at > 0 && is(&words, at - 1, "DEFAULT")) {
            words.remove(at);
            slot.get_or_insert(at);
        } else if !column.nullable && is(&words, at, "DEFAULT") && is(&words, at + 1, "NULL") {
            words.drain(at..at + 2);
        } else {
            at += 1;
        }
    }
    let slot = slot.unwrap_or_else(|| {
        words
            .iter()
            .position(|word| {
                ["DEFAULT", "ON", "AUTO_INCREMENT", "COMMENT"]
                    .iter()
                    .any(|key| word.eq_ignore_ascii_case(key))
                    || word.starts_with("/*")
            })
            .unwrap_or(words.len())
    });
    let null_sql = if column.nullable { "NULL" } else { "NOT NULL" };
    words.insert(slot.min(words.len()), null_sql.to_string());
    Ok(format!("{new_type} {}", words.join(" ")))
}

fn is_text_type(data_type: &str) -> bool {
    let lower = data_type.trim().to_ascii_lowercase();
    [
        "char",
        "varchar",
        "tinytext",
        "text",
        "mediumtext",
        "longtext",
        "enum",
        "set",
    ]
    .iter()
    .any(|name| {
        lower
            .strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['(', ' ']))
    })
}

/// Split on spaces outside quotes, parentheses, and comments.
fn top_level_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(open) = quote {
            word.push(ch);
            if ch == '\\' && open != '`' {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            } else if open == '*' {
                if ch == '*' && chars.peek() == Some(&'/') {
                    word.push('/');
                    chars.next();
                    quote = None;
                }
            } else if ch == open {
                // A doubled quote stays inside the string.
                if chars.peek() == Some(&open) {
                    word.push(open);
                    chars.next();
                } else {
                    quote = None;
                }
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => {
                quote = Some(ch);
                word.push(ch);
            }
            '/' if chars.peek() == Some(&'*') => {
                quote = Some('*');
                word.push(ch);
                word.push('*');
                chars.next();
            }
            '(' => {
                depth += 1;
                word.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                word.push(ch);
            }
            ch if ch.is_whitespace() && depth == 0 => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            _ => word.push(ch),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// SQLite alters in place only. The old rebuild re-created the table from the
/// draft and lost DEFAULT, CHECK, UNIQUE, COLLATE, AUTOINCREMENT, STRICT,
/// foreign keys, indexes, and triggers (R-03). Until it rebuilds from the
/// original `CREATE TABLE`, anything that needs a rebuild is refused.
fn plan_sqlite(draft: &TableDefinition) -> DdlPlan {
    let mut warnings = sqlite_refusals(draft);
    let mut statements = Vec::new();
    let table = qualify(DdlDialect::Sqlite, draft);
    for column in draft.columns.iter().filter(|column| !column.drop) {
        match &column.original {
            None => {
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
            Some(original) if !original.name.eq_ignore_ascii_case(&column.name) => {
                statements.push(format!(
                    "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                    quote(DdlDialect::Sqlite, &original.name),
                    quote(DdlDialect::Sqlite, &column.name)
                ));
            }
            Some(_) => {}
        }
    }
    statements.extend(index_statements(DdlDialect::Sqlite, draft, true));
    if let Some(rename) = rename_sql(DdlDialect::Sqlite, draft) {
        statements.push(rename);
    }
    // SQLite DDL is transactional: a failed step leaves nothing half-applied.
    let transactional = statements.len() > 1;
    if transactional {
        statements.insert(0, "BEGIN".into());
        statements.push("COMMIT".into());
    }
    finish(
        DdlDialect::Sqlite,
        statements,
        warnings,
        transactional,
        false,
        None,
    )
}

const SQLITE_LIMIT: &str =
    "On SQLite, only adding columns, renaming the table or a column, and index changes can be applied for now.";

fn sqlite_refusals(draft: &TableDefinition) -> Vec<String> {
    let mut refused = Vec::new();
    for column in &draft.columns {
        let Some(original) = &column.original else {
            continue;
        };
        let name = &original.name;
        if column.drop {
            refused.push(format!("Column {name} cannot be dropped. {SQLITE_LIMIT}"));
            continue;
        }
        if !original
            .data_type
            .eq_ignore_ascii_case(column.data_type.trim())
        {
            refused.push(format!(
                "The type of column {name} cannot be changed. {SQLITE_LIMIT}"
            ));
        }
        if original.nullable != column.nullable {
            refused.push(format!(
                "NOT NULL on column {name} cannot be changed. {SQLITE_LIMIT}"
            ));
        }
    }
    let new_pk = draft
        .columns
        .iter()
        .any(|column| column.original.is_none() && !column.drop && column.pk_position > 0);
    if pk_changed(draft) || new_pk {
        refused.push(format!("The primary key cannot be changed. {SQLITE_LIMIT}"));
    }
    if draft.foreign_keys.iter().any(fk_changes) {
        refused.push(format!(
            "Foreign keys cannot be added, changed, or dropped. {SQLITE_LIMIT}"
        ));
    }
    refused
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

fn validate_columns(draft: &TableDefinition, warnings: &mut Vec<String>) {
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

fn validate_indexes(draft: &TableDefinition, warnings: &mut Vec<String>) {
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

fn validate_foreign_keys(draft: &TableDefinition, warnings: &mut Vec<String>) {
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

/// Old names of live columns that are being renamed.
fn renamed_column_names(draft: &TableDefinition) -> Vec<String> {
    draft
        .columns
        .iter()
        .filter(|column| !column.drop)
        .filter_map(|column| column.original.as_ref())
        .map(|original| original.name.clone())
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
        definition: column
            .original
            .as_ref()
            .and_then(|original| original.definition.clone()),
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
    fn sqlite_add_and_renames_alter_in_place() {
        let add = table("orders", vec![existing("id"), text_column("note", true)]);
        let plan = plan_table_alter(DdlDialect::Sqlite, &add);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(!plan.requires_rebuild);
        assert_eq!(
            plan.statements,
            vec!["ALTER TABLE \"orders\" ADD COLUMN \"note\" text"]
        );

        let mut renamed = existing("title");
        renamed.name = "headline".into();
        let mut draft = table(
            "orders",
            vec![existing("id"), renamed, text_column("note", true)],
        );
        draft.rename_to = Some("purchases".into());
        let plan = plan_table_alter(DdlDialect::Sqlite, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert!(plan.transactional);
        assert_eq!(
            plan.statements,
            vec![
                "BEGIN",
                "ALTER TABLE \"orders\" RENAME COLUMN \"title\" TO \"headline\"",
                "ALTER TABLE \"orders\" ADD COLUMN \"note\" text",
                "ALTER TABLE \"orders\" RENAME TO \"purchases\"",
                "COMMIT",
            ]
        );
    }

    #[test]
    fn renaming_an_indexed_column_keeps_the_index_valid() {
        let mut renamed = existing("title");
        renamed.name = "headline".into();
        let mut draft = table("orders", vec![existing("id"), renamed]);
        draft.indexes.push(IndexDraft::loaded(
            "orders_title",
            vec!["title".into()],
            false,
            false,
        ));
        let plan = plan_table_alter(DdlDialect::Postgres, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert_eq!(
            plan.statements,
            vec!["ALTER TABLE \"public\".\"orders\" RENAME COLUMN \"title\" TO \"headline\""]
        );
    }

    fn mysql_column(data_type: &str, nullable: bool, definition: Option<&str>) -> ColumnDraft {
        let mut column = ColumnDraft::loaded("c", data_type, nullable, 0);
        if let Some(original) = column.original.as_mut() {
            original.definition = definition.map(str::to_string);
        }
        column
    }

    fn mysql_modify(column: ColumnDraft) -> Result<String, String> {
        let original = column.original.clone().expect("original");
        mysql_modify_definition(&original, &column)
    }

    #[test]
    fn mysql_modify_keeps_every_attribute_it_does_not_change() {
        let mut column = mysql_column(
            "varchar(20)",
            false,
            Some("varchar(20) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x y' COMMENT 'it''s (the) name'"),
        );
        column.data_type = "varchar(40)".into();
        assert_eq!(
            mysql_modify(column).as_deref(),
            Ok("varchar(40) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x y' COMMENT 'it''s (the) name'")
        );

        let mut id = mysql_column("int", false, Some("int NOT NULL AUTO_INCREMENT"));
        id.data_type = "bigint".into();
        assert_eq!(
            mysql_modify(id).as_deref(),
            Ok("bigint NOT NULL AUTO_INCREMENT")
        );

        let mut ts = mysql_column(
            "timestamp",
            true,
            Some("timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched'"),
        );
        ts.nullable = false;
        assert_eq!(
            mysql_modify(ts).as_deref(),
            Ok("timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched'")
        );
    }

    #[test]
    fn mysql_modify_moves_null_and_drops_what_no_longer_applies() {
        // Nullable column without an explicit NULL; DEFAULT NULL goes with NOT NULL.
        let mut column = mysql_column("int", true, Some("int DEFAULT NULL COMMENT 'n'"));
        column.nullable = false;
        assert_eq!(
            mysql_modify(column).as_deref(),
            Ok("int NOT NULL COMMENT 'n'")
        );

        let mut column = mysql_column("int", false, Some("int NOT NULL DEFAULT '5'"));
        column.nullable = true;
        assert_eq!(mysql_modify(column).as_deref(), Ok("int NULL DEFAULT '5'"));

        // A charset makes no sense on a number.
        let mut column = mysql_column(
            "varchar(10)",
            true,
            Some("varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL"),
        );
        column.data_type = "int".into();
        assert_eq!(mysql_modify(column).as_deref(), Ok("int NULL DEFAULT NULL"));

        // Expression defaults and generated columns keep their parentheses.
        let mut column = mysql_column(
            "int",
            true,
            Some("int GENERATED ALWAYS AS ((`a` + 1)) VIRTUAL"),
        );
        column.nullable = false;
        assert_eq!(
            mysql_modify(column).as_deref(),
            Ok("int GENERATED ALWAYS AS ((`a` + 1)) VIRTUAL NOT NULL")
        );
    }

    #[test]
    fn mysql_modify_without_a_definition_is_refused() {
        let mut column = mysql_column("int", true, None);
        column.nullable = false;
        let mut draft = table("orders", vec![column]);
        draft.schema = None;
        draft.database = Some("app".into());
        let plan = plan_table_alter(DdlDialect::Mysql, &draft);
        assert!(!plan.applyable());
        assert!(
            plan.warnings.iter().any(|w| w.contains("Reload")),
            "{plan:?}"
        );

        // The definition must start with the type it was read with.
        let mut column = mysql_column("int", true, Some("bigint DEFAULT NULL"));
        column.nullable = false;
        assert!(mysql_modify(column).is_err());
    }

    #[test]
    fn mysql_plan_shows_the_full_modify() {
        let mut column = mysql_column("int", false, Some("int NOT NULL AUTO_INCREMENT"));
        column.data_type = "bigint".into();
        let mut draft = table("orders", vec![column]);
        draft.schema = None;
        draft.database = Some("app".into());
        let plan = plan_table_alter(DdlDialect::Mysql, &draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        assert_eq!(
            plan.statements,
            vec!["ALTER TABLE `app`.`orders` MODIFY COLUMN `c` bigint NOT NULL AUTO_INCREMENT"]
        );
    }

    #[test]
    fn sqlite_refuses_every_change_that_needs_a_rebuild() {
        let mut typed = existing("note");
        typed.data_type = "integer".into();
        let mut not_null = existing("note");
        not_null.nullable = false;
        let mut dropped = existing("note");
        dropped.drop = true;
        let mut keyed = existing("note");
        keyed.pk_position = 1;
        let mut new_key = text_column("code", true);
        new_key.pk_position = 1;
        let mut with_fk = table("orders", vec![existing("id"), existing("note")]);
        with_fk.foreign_keys.push(ForeignKeyDraft {
            name: "orders_note_fk".into(),
            columns: vec!["note".into()],
            ref_schema: None,
            ref_table: "notes".into(),
            ref_columns: vec!["id".into()],
            original: None,
            drop: false,
        });
        let drafts = [
            table("orders", vec![existing("id"), typed]),
            table("orders", vec![existing("id"), not_null]),
            table("orders", vec![existing("id"), dropped]),
            table("orders", vec![existing("id"), keyed]),
            table("orders", vec![existing("id"), new_key]),
            with_fk,
        ];
        for draft in &drafts {
            let plan = plan_table_alter(DdlDialect::Sqlite, draft);
            assert!(!plan.applyable(), "{draft:?} -> {plan:?}");
            assert!(plan.statements.is_empty(), "{plan:?}");
            assert!(
                plan.warnings.iter().any(|w| w.contains("On SQLite")),
                "{plan:?}"
            );
        }
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
