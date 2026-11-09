//! Structure drafts and plans. Serde shapes are part of the IPC contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ColumnOriginal {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub pk_position: u32,
    /// MySQL: the column definition after its name, as `SHOW CREATE TABLE`
    /// prints it. MODIFY is rebuilt from it so DEFAULT, ON UPDATE,
    /// AUTO_INCREMENT, COMMENT, charset, and collation survive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// Default expression as the catalog prints it.
    #[serde(default)]
    pub default_value: Option<String>,
    /// Identity or auto-increment: `always` / `by default` (Postgres),
    /// `auto_increment` (MySQL), `autoincrement` (SQLite).
    #[serde(default)]
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ColumnDraft {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub pk_position: u32,
    #[serde(default)]
    pub original: Option<ColumnOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
    /// Default expression. `None` means no default.
    #[serde(default)]
    pub default_value: Option<String>,
    /// See [`ColumnOriginal::identity`].
    #[serde(default)]
    pub identity: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct IndexOriginal {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub primary: bool,
    /// The full `CREATE INDEX` as the engine prints it (`pg_get_indexdef`,
    /// `sqlite_master.sql`), so partial, expression, and INCLUDE indexes
    /// are replayed as they are.
    #[serde(default)]
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct IndexDraft {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub primary: bool,
    /// Postgres `CREATE INDEX CONCURRENTLY`. Cannot share a transaction.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub concurrent: bool,
    #[serde(default)]
    pub original: Option<IndexOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
    /// See [`IndexOriginal::definition`]. Used to create the index when its
    /// columns were not edited.
    #[serde(default)]
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyOriginal {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    /// `CASCADE`, `SET NULL`, `SET DEFAULT`, `RESTRICT`. `None` is NO ACTION.
    #[serde(default)]
    pub on_delete: Option<String>,
    #[serde(default)]
    pub on_update: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
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
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
    #[serde(default)]
    pub on_delete: Option<String>,
    #[serde(default)]
    pub on_update: Option<String>,
}

/// A CHECK or UNIQUE constraint that is not an index of its own.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ConstraintOriginal {
    /// `check` or `unique`.
    pub kind: String,
    /// Empty for an unnamed SQLite constraint.
    #[serde(default)]
    pub name: String,
    /// UNIQUE columns.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<String>,
    /// CHECK expression, without the `CHECK` keyword.
    #[serde(default)]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ConstraintDraft {
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<String>,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub original: Option<ConstraintOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
}

/// A trigger on the table, as the engine stores it. Read only: SQLite
/// replays it after a table rebuild.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TriggerDefinition {
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableDefinition {
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default = "default_table_kind")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub kind: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<ColumnDraft>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub indexes: Vec<IndexDraft>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub foreign_keys: Vec<ForeignKeyDraft>,
    /// CHECK and UNIQUE constraints (UNIQUE ones that are only an index
    /// on this engine stay in `indexes`).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub constraints: Vec<ConstraintDraft>,
    /// Triggers on the table (SQLite: replayed after a rebuild).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub triggers: Vec<TriggerDefinition>,
    /// SQLite `sqlite_master.sql` captured when the definition was read.
    #[serde(default)]
    pub schema_revision: Option<String>,
    #[serde(default)]
    pub validator: Option<String>,
    #[serde(default)]
    pub original_validator: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub create: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
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
                ..ColumnOriginal::default()
            }),
            ..Self::default()
        }
    }

    /// Set the default and identity on the column and its original.
    pub fn with_default(mut self, default_value: Option<String>, identity: Option<String>) -> Self {
        if let Some(original) = self.original.as_mut() {
            original.default_value.clone_from(&default_value);
            original.identity.clone_from(&identity);
        }
        self.default_value = default_value;
        self.identity = identity;
        self
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
            original: Some(IndexOriginal {
                name,
                columns,
                unique,
                primary,
                definition: None,
            }),
            ..Self::default()
        }
    }

    /// Set the engine's `CREATE INDEX` text on the index and its original.
    pub fn with_definition(mut self, definition: Option<String>) -> Self {
        if let Some(original) = self.original.as_mut() {
            original.definition.clone_from(&definition);
        }
        self.definition = definition;
        self
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
                ..ForeignKeyOriginal::default()
            }),
            ..Self::default()
        }
    }

    /// Set ON DELETE / ON UPDATE. `NO ACTION` is stored as `None`.
    pub fn with_actions(mut self, on_delete: Option<String>, on_update: Option<String>) -> Self {
        let on_delete = clean_action(on_delete.as_deref());
        let on_update = clean_action(on_update.as_deref());
        if let Some(original) = self.original.as_mut() {
            original.on_delete.clone_from(&on_delete);
            original.on_update.clone_from(&on_update);
        }
        self.on_delete = on_delete;
        self.on_update = on_update;
        self
    }
}

/// Upper-case referential action; `None` for empty or `NO ACTION`.
pub fn clean_action(action: Option<&str>) -> Option<String> {
    action
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .map(|text| text.to_ascii_uppercase())
        .filter(|text| !text.is_empty() && text != "NO ACTION")
}

impl ConstraintDraft {
    /// A constraint just read from the catalog.
    pub fn loaded(
        kind: &str,
        name: impl Into<String>,
        columns: Vec<String>,
        expression: Option<String>,
    ) -> Self {
        let original = ConstraintOriginal {
            kind: kind.into(),
            name: name.into(),
            columns,
            expression,
        };
        Self {
            kind: original.kind.clone(),
            name: original.name.clone(),
            columns: original.columns.clone(),
            expression: original.expression.clone(),
            original: Some(original),
            drop: false,
        }
    }
}

impl Default for TableDefinition {
    fn default() -> Self {
        Self::loaded(None, None, String::new(), "table")
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
            constraints: Vec::new(),
            triggers: Vec::new(),
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct RenamePair {
    /// Name on the source side (the name to end with).
    pub from: String,
    /// Name on the target side (the name it has now).
    pub to: String,
    /// Set for a column pair: the source-side table the columns belong to.
    /// `None` pairs two tables.
    #[serde(default)]
    pub table: Option<String>,
    /// The user said these two columns are not a rename. Stops the guess.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub keep_apart: bool,
}

impl RenamePair {
    pub fn tables(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            table: None,
            keep_apart: false,
        }
    }

    pub fn columns(
        table: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            table: Some(table.into()),
            keep_apart: false,
        }
    }
}

/// Which parts of a schema diff to apply. Drops are opt-in: a dropped
/// table, column, index, foreign key, or constraint is applied only when
/// it is listed in `drops`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct DiffSelection {
    /// Creates and alters to leave out, by change name.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub skip: Vec<String>,
    /// Drops to apply.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drops: Vec<DropKey>,
}

/// One drop in a schema diff. `kind` is `table`, `column`, `index`,
/// `foreignKey`, or `constraint`; for `table`, `name` is the table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DropKey {
    pub table: String,
    pub kind: String,
    pub name: String,
}

/// A drop the diff found, and whether the selection includes it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DropItem {
    pub kind: String,
    pub name: String,
    pub included: bool,
}

/// A column pair the diff treats as a rename.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ColumnRename {
    /// Source-side name (the name to end with).
    pub from: String,
    /// Target-side name (the name it has now).
    pub to: String,
    /// Paired by the heuristic, not by the user.
    pub guessed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaChange {
    pub name: String,
    pub action: String,
    pub plan: DdlPlan,
    pub draft: TableDefinition,
    /// Applied with the migration: false for a skipped change, or a table
    /// drop that was not chosen.
    pub selected: bool,
    /// Drops inside this change (for `drop`, the table itself).
    pub drops: Vec<DropItem>,
    /// Column renames in this change.
    pub column_renames: Vec<ColumnRename>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaMigration {
    pub changes: Vec<SchemaChange>,
    pub warnings: Vec<String>,
}
