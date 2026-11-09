//! Which database (and schema) each write in a batch lands in.
//!
//! The write guard marks databases production one by one, so it needs the
//! database a statement writes to, not only the connection. Names are read
//! off the AST:
//!
//! - Postgres: `t` and `s.t` are in the current database, `d.s.t` names
//!   database `d` (Postgres refuses a cross-database name, but the guard
//!   still treats it as that database).
//! - MySQL: `t` is in the current database, `d.t` is in `d`, and `USE d`
//!   changes the current database for the statements after it.
//! - SQLite: one file, so every name is the current database.
//!
//! A write whose target cannot be named (text that does not parse, a write
//! hidden in a function call, `CALL`, `DO`, a statement kind this module does
//! not read, or an unqualified name with no current database) sets
//! [`WriteTargets::unresolved`]. The guard fails closed on it.

use std::ops::ControlFlow;

use sqlparser::ast::{
    CopySource, FromTable, Ident, ObjectName, ObjectNamePart, ObjectType, SchemaName, Statement,
    TableFactor, TableObject, TableWithJoins, Use, Visit, Visitor,
};
use sqlparser::parser::Parser;

use super::{classify, split_statements, SqlDialect};

/// One database a write lands in. `schema` is the Postgres schema when the
/// name carries one; `None` means the search path decides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WriteTarget {
    pub database: String,
    pub schema: Option<String>,
}

/// Every database the writes in a batch land in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteTargets {
    /// Sorted, no duplicates.
    pub targets: Vec<WriteTarget>,
    /// Some write's target could not be named.
    pub unresolved: bool,
}

impl WriteTargets {
    /// Nothing could be named: the guard treats it as the most dangerous database.
    pub fn unresolved() -> Self {
        Self {
            targets: Vec::new(),
            unresolved: true,
        }
    }

    /// The current database, or unresolved when there is none.
    pub fn current(database: Option<&str>) -> Self {
        match clean(database) {
            Some(database) => Self {
                targets: vec![WriteTarget {
                    database,
                    schema: None,
                }],
                unresolved: false,
            },
            None => Self::unresolved(),
        }
    }

    /// One named database.
    pub fn database(database: &str, schema: Option<&str>) -> Self {
        let mut out = Self::default();
        match clean(Some(database)) {
            Some(database) => out.push(WriteTarget {
                database,
                schema: clean(schema),
            }),
            None => out.unresolved = true,
        }
        out
    }

    pub fn push(&mut self, target: WriteTarget) {
        if let Err(at) = self.targets.binary_search(&target) {
            self.targets.insert(at, target);
        }
    }

    pub fn extend(&mut self, other: WriteTargets) {
        self.unresolved |= other.unresolved;
        for target in other.targets {
            self.push(target);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && !self.unresolved
    }
}

fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Databases the writes in `sql` land in. Reads are skipped. `current` is
/// the database the session runs in (`None` when it has none).
pub fn write_targets(sql: &str, dialect: SqlDialect, current: Option<&str>) -> WriteTargets {
    let mut current = clean(current);
    let mut out = WriteTargets::default();
    for stmt in split_statements(sql, dialect) {
        let parsed = parse(&stmt, dialect);
        // `USE` changes where the statements after it write.
        if let Some(statements) = &parsed {
            for statement in statements {
                if let Statement::Use(target) = statement {
                    current = use_database(target, dialect);
                }
            }
        }
        if classify(&stmt, dialect).is_read_like() {
            continue;
        }
        let Some(statements) = parsed else {
            out.unresolved = true;
            continue;
        };
        for statement in &statements {
            let mut scan = TargetScan {
                dialect,
                current: current.as_deref(),
                out: WriteTargets::default(),
            };
            let _ = statement.visit(&mut scan);
            let mut found = scan.out;
            // A query writes through a data-modifying CTE or a function.
            // Without a named write inside it, nothing can be resolved.
            if matches!(statement, Statement::Query(_)) && found.targets.is_empty() {
                found.unresolved = true;
            }
            out.extend(found);
        }
    }
    out
}

fn parse(sql: &str, dialect: SqlDialect) -> Option<Vec<Statement>> {
    let boxed = dialect.parser_dialect();
    match Parser::parse_sql(&*boxed, sql) {
        Ok(statements) => Some(statements),
        Err(_) if !matches!(dialect, SqlDialect::Generic) => {
            Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql).ok()
        }
        Err(_) => None,
    }
}

fn use_database(target: &Use, dialect: SqlDialect) -> Option<String> {
    let name = match target {
        Use::Object(name) | Use::Database(name) | Use::Schema(name) => name,
        _ => return None,
    };
    match idents(name, dialect)?.as_slice() {
        [database] => Some(database.clone()),
        _ => None,
    }
}

/// How an object name maps to a database.
#[derive(Clone, Copy)]
enum NameKind {
    /// Table, view, index, sequence, …
    Object,
    /// `CREATE` / `DROP DATABASE`.
    Database,
    /// `CREATE` / `DROP SCHEMA`: a database on MySQL, a schema on Postgres.
    Schema,
}

struct TargetScan<'a> {
    dialect: SqlDialect,
    current: Option<&'a str>,
    out: WriteTargets,
}

impl TargetScan<'_> {
    fn name(&mut self, name: &ObjectName, kind: NameKind) {
        match name_target(name, kind, self.dialect, self.current) {
            Some(target) => self.out.push(target),
            None => self.out.unresolved = true,
        }
    }

    fn relation(&mut self, relation: &TableFactor) {
        match relation {
            TableFactor::Table { name, args, .. } if args.is_none() => {
                self.name(name, NameKind::Object)
            }
            _ => self.out.unresolved = true,
        }
    }

    /// The table and, for MySQL multi-table writes, every joined table.
    fn tables(&mut self, tables: &[TableWithJoins]) {
        for table in tables {
            self.relation(&table.relation);
            for join in &table.joins {
                self.relation(&join.relation);
            }
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            // Nested writes are visited on their own; the query alone names nothing.
            Statement::Query(_) => {}
            Statement::Insert(insert) => match &insert.table {
                TableObject::TableName(name) => self.name(name, NameKind::Object),
                _ => self.out.unresolved = true,
            },
            Statement::Update { table, .. } => self.tables(std::slice::from_ref(table)),
            Statement::Delete(delete) => {
                for name in &delete.tables {
                    self.name(name, NameKind::Object);
                }
                match &delete.from {
                    FromTable::WithFromKeyword(from) | FromTable::WithoutKeyword(from) => {
                        if from.is_empty() {
                            self.out.unresolved = true;
                        }
                        self.tables(from);
                    }
                }
            }
            Statement::Merge { table, .. } => self.relation(table),
            Statement::CreateTable(create) => self.name(&create.name, NameKind::Object),
            Statement::CreateView { name, .. }
            | Statement::AlterTable { name, .. }
            | Statement::AlterIndex { name, .. }
            | Statement::AlterView { name, .. }
            | Statement::CreateSequence { name, .. }
            | Statement::CreateType { name, .. } => self.name(name, NameKind::Object),
            Statement::CreateIndex(create) => self.name(&create.table_name, NameKind::Object),
            Statement::Drop {
                object_type,
                names,
                table,
                ..
            } => {
                let kind = match object_type {
                    ObjectType::Database => NameKind::Database,
                    ObjectType::Schema => NameKind::Schema,
                    _ => NameKind::Object,
                };
                for name in names {
                    self.name(name, kind);
                }
                if let Some(table) = table {
                    self.name(table, NameKind::Object);
                }
            }
            Statement::Truncate { table_names, .. } => {
                for target in table_names {
                    self.name(&target.name, NameKind::Object);
                }
            }
            Statement::CreateSchema { schema_name, .. } => match schema_name {
                SchemaName::Simple(name) | SchemaName::NamedAuthorization(name, _) => {
                    self.name(name, NameKind::Schema)
                }
                SchemaName::UnnamedAuthorization(_) => match clean(self.current) {
                    Some(database) => self.out.push(WriteTarget {
                        database,
                        schema: None,
                    }),
                    None => self.out.unresolved = true,
                },
            },
            Statement::CreateDatabase { db_name, .. } => self.name(db_name, NameKind::Database),
            Statement::RenameTable(renames) => {
                for rename in renames {
                    self.name(&rename.old_name, NameKind::Object);
                    self.name(&rename.new_name, NameKind::Object);
                }
            }
            Statement::Copy {
                source: CopySource::Table { table_name, .. },
                to: false,
                ..
            } => self.name(table_name, NameKind::Object),
            // Everything else that writes (CALL, DO, GRANT, COPY … FROM a
            // query, a statement this module does not read) is unresolved.
            _ => self.out.unresolved = true,
        }
    }
}

impl Visitor for TargetScan<'_> {
    type Break = ();

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        self.statement(statement);
        ControlFlow::Continue(())
    }
}

/// `None` when the name cannot be placed: a function part, too many parts,
/// or an unqualified name with no current database.
fn name_target(
    name: &ObjectName,
    kind: NameKind,
    dialect: SqlDialect,
    current: Option<&str>,
) -> Option<WriteTarget> {
    let parts = idents(name, dialect)?;
    let current = || clean(current);
    let target = |database: String, schema: Option<String>| WriteTarget { database, schema };
    if dialect == SqlDialect::Sqlite {
        // One file; attached schemas are not marked separately.
        return Some(target(current().unwrap_or_else(|| "main".into()), None));
    }
    let mysql = dialect == SqlDialect::Mysql;
    match (kind, parts.as_slice()) {
        (NameKind::Database, [database]) => Some(target(database.clone(), None)),
        (NameKind::Schema, [database]) if mysql => Some(target(database.clone(), None)),
        (NameKind::Schema, [schema]) => Some(target(current()?, Some(schema.clone()))),
        (NameKind::Schema, [database, schema]) if !mysql => {
            Some(target(database.clone(), Some(schema.clone())))
        }
        (NameKind::Object, [_]) => Some(target(current()?, None)),
        (NameKind::Object, [database, _]) if mysql => Some(target(database.clone(), None)),
        (NameKind::Object, [schema, _]) => Some(target(current()?, Some(schema.clone()))),
        (NameKind::Object, [database, schema, _]) if !mysql => {
            Some(target(database.clone(), Some(schema.clone())))
        }
        _ => None,
    }
}

fn idents(name: &ObjectName, dialect: SqlDialect) -> Option<Vec<String>> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident_value(ident, dialect)),
            _ => None,
        })
        .collect()
}

/// Postgres folds an unquoted name to lower case.
fn ident_value(ident: &Ident, dialect: SqlDialect) -> String {
    if dialect == SqlDialect::Postgres && ident.quote_style.is_none() {
        ident.value.to_lowercase()
    } else {
        ident.value.clone()
    }
}

#[cfg(test)]
mod tests;
