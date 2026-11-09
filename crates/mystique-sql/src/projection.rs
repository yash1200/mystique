//! Which t.is_some_and(|a| a.eq_ignore_ascii_case(one))ble column each output column of a plain single-table SELECT is.
//!
//! Used only where the driver does not report column provenance (SQLite via
//! rusqlite). Anything that is not one table, read column by column, is
//! `None`: joins, set operations, CTEs, DISTINCT, GROUP BY, HAVING, window
//! clauses and subqueries in FROM. An expression in the select list is
//! [`ProjectedItem::Other`], so a key computed by an expression never counts.

use sqlparser::ast::{
    Expr, GroupByExpr, ObjectName, ObjectNamePart, Query, SelectItem,
    SelectItemQualifiedWildcardKind, SetExpr, Statement, TableFactor,
};
use sqlparser::parser::Parser;

use crate::SqlDialect;

/// The FROM table and the select list, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleTableSelect {
    /// The qualifier before the table name, if any (`main`, `public`, a database).
    pub qualifier: Option<String>,
    pub table: String,
    pub items: Vec<ProjectedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectedItem {
    /// `*` or `t.*`: every table column, in table order.
    Wildcard,
    /// A bare column, possibly aliased.
    Column(String),
    /// Anything else: an expression, a literal, a function call.
    Other,
}

pub fn single_table_select(sql: &str, dialect: SqlDialect) -> Option<SingleTableSelect> {
    let parser = dialect.parser_dialect();
    let mut statements = Parser::parse_sql(&*parser, sql).ok()?;
    if statements.len() != 1 {
        return None;
    }
    let Statement::Query(query) = statements.remove(0) else {
        return None;
    };
    from_query(&query)
}

fn from_query(query: &Query) -> Option<SingleTableSelect> {
    if query.with.is_some() || !query.pipe_operators.is_empty() {
        return None;
    }
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    if select.distinct.is_some()
        || select.having.is_some()
        || select.into.is_some()
        || !select.lateral_views.is_empty()
        || !select.named_window.is_empty()
        || select.qualify.is_some()
        || select.connect_by.is_some()
    {
        return None;
    }
    match &select.group_by {
        GroupByExpr::Expressions(exprs, modifiers) if exprs.is_empty() && modifiers.is_empty() => {}
        _ => return None,
    }
    if select.from.len() != 1 || !select.from[0].joins.is_empty() {
        return None;
    }
    let TableFactor::Table {
        name, alias, args, ..
    } = &select.from[0].relation
    else {
        return None;
    };
    if args.is_some() {
        return None;
    }
    let mut parts = name_parts(name)?;
    let table = parts.pop()?;
    if parts.len() > 1 {
        return None;
    }
    let qualifier = parts.pop();
    let alias = alias.as_ref().map(|a| a.name.value.clone());

    let refers_to_table = |qual: &[String]| -> bool {
        match qual {
            [one] => {
                alias
                    .as_deref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(one))
                    || (alias.is_none() && one.eq_ignore_ascii_case(&table))
            }
            [schema, name] => {
                alias.is_none()
                    && name.eq_ignore_ascii_case(&table)
                    && qualifier
                        .as_deref()
                        .is_some_and(|q| q.eq_ignore_ascii_case(schema))
            }
            _ => false,
        }
    };

    let mut items = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        let projected = match item {
            SelectItem::Wildcard(_) => ProjectedItem::Wildcard,
            SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(name), _) => {
                let qual = name_parts(name)?;
                if !refers_to_table(&qual) {
                    return None;
                }
                ProjectedItem::Wildcard
            }
            SelectItem::QualifiedWildcard(_, _) => return None,
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => match expr {
                Expr::Identifier(ident) => ProjectedItem::Column(ident.value.clone()),
                Expr::CompoundIdentifier(idents) if idents.len() >= 2 => {
                    let (column, qual) = idents.split_last()?;
                    let qual: Vec<String> = qual.iter().map(|i| i.value.clone()).collect();
                    if refers_to_table(&qual) {
                        ProjectedItem::Column(column.value.clone())
                    } else {
                        ProjectedItem::Other
                    }
                }
                _ => ProjectedItem::Other,
            },
        };
        items.push(projected);
    }
    Some(SingleTableSelect {
        qualifier,
        table,
        items,
    })
}

fn name_parts(name: &ObjectName) -> Option<Vec<String>> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(sql: &str) -> Option<Vec<ProjectedItem>> {
        single_table_select(sql, SqlDialect::Sqlite).map(|s| s.items)
    }

    #[test]
    fn plain_reads_name_their_columns() {
        let select = single_table_select(
            "SELECT id, body AS content, length(body), d.title FROM docs d WHERE id > 3 ORDER BY id LIMIT 10",
            SqlDialect::Sqlite,
        )
        .expect("single table");
        assert_eq!(select.table, "docs");
        assert_eq!(select.qualifier, None);
        assert_eq!(
            select.items,
            vec![
                ProjectedItem::Column("id".into()),
                ProjectedItem::Column("body".into()),
                ProjectedItem::Other,
                ProjectedItem::Column("title".into()),
            ]
        );
        assert_eq!(
            items("SELECT * FROM docs"),
            Some(vec![ProjectedItem::Wildcard])
        );
        assert_eq!(
            items("SELECT docs.*, 1 FROM main.docs"),
            Some(vec![ProjectedItem::Wildcard, ProjectedItem::Other])
        );
    }

    #[test]
    fn anything_but_one_table_is_refused() {
        for sql in [
            "SELECT a.id FROM a JOIN b ON a.id = b.id",
            "SELECT id FROM a, b",
            "SELECT DISTINCT id FROM a",
            "SELECT id FROM a GROUP BY id",
            "SELECT id FROM a UNION SELECT id FROM b",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "SELECT id FROM (SELECT id FROM a) s",
            "INSERT INTO a VALUES (1)",
            "SELECT 1; SELECT 2",
            "SELECT b.id FROM a",
        ] {
            let parsed = single_table_select(sql, SqlDialect::Sqlite);
            let refused = parsed
                .as_ref()
                .is_none_or(|s| s.items.iter().all(|i| *i == ProjectedItem::Other));
            assert!(refused, "{sql}: {parsed:?}");
        }
    }
}
