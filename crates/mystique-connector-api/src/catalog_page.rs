//! Lazy, paged catalog: one level of the explorer tree at a time.
//!
//! [`crate::ConnectorSession::list_catalog`] returns every database and
//! object at once, which is fine for a few thousand tables and wrong for a
//! Redis keyspace or an Elasticsearch cluster. `list_catalog_children` asks
//! for the children of one node, a page at a time, behind an opaque cursor.
//!
//! Levels, by how much of [`CatalogPath`] is set:
//! - nothing: databases (kind `database`);
//! - `database`: groups (kind `schema`) on a `schema-tree` layout, else objects;
//! - `database` + `schema`: objects;
//! - `prefix` (any depth): the plugin's own nesting (Redis `a:b:` key folders).
//!
//! The default implementation derives every level from `list_catalog`, so the
//! four existing plugins answer it without code. Engines that set
//! [`crate::EngineCapabilities::supports_catalog_paging`] page natively.

use serde::{Deserialize, Serialize};

use crate::catalog::{CatalogDatabase, CatalogObject};
use crate::error::{ConnectorError, ConnectorResult};

/// Default and maximum page sizes for one level.
pub const CATALOG_PAGE_DEFAULT: u32 = 500;
pub const CATALOG_PAGE_MAX: u32 = 5_000;

/// Kind of a database node.
pub const KIND_DATABASE: &str = "database";
/// Kind of a schema (group) node.
pub const KIND_SCHEMA: &str = "schema";

/// Which node's children to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct CatalogPath {
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    /// Plugin-defined nesting below the schema (key folders, index aliases).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub prefix: Vec<String>,
}

/// One node in the explorer tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CatalogNode {
    pub name: String,
    /// `database`, `schema`, or one of the plugin's object kinds.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub schema: Option<String>,
    /// The node can be expanded (databases, schemas, key folders).
    pub has_children: bool,
    /// Engine estimate, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub row_count: Option<u64>,
    /// Short label for the picker (size, owner, `system`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub meta: Option<String>,
}

/// One page of children.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub nodes: Vec<CatalogNode>,
    /// Pass back as `cursor` for the next page. `None`: no more children.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub next_cursor: Option<String>,
}

/// IPC request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ListCatalogChildrenRequest {
    pub session_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub parent: CatalogPath,
    /// The previous page's `next_cursor`.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size; clamped to [`CATALOG_PAGE_MAX`].
    #[serde(default)]
    pub limit: Option<u32>,
    /// Case-insensitive name filter, applied by the engine where it can
    /// (`SCAN MATCH`, `LIKE`) so a huge level is searched server-side.
    #[serde(default)]
    pub filter: Option<String>,
}

/// Plugin input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListCatalogChildrenInput {
    pub parent: CatalogPath,
    pub cursor: Option<String>,
    pub limit: u32,
    pub filter: Option<String>,
}

impl ListCatalogChildrenRequest {
    pub fn into_input(self) -> ListCatalogChildrenInput {
        ListCatalogChildrenInput {
            parent: CatalogPath {
                database: non_empty(self.parent.database),
                schema: non_empty(self.parent.schema),
                prefix: self.parent.prefix,
            },
            cursor: non_empty(self.cursor),
            limit: self
                .limit
                .unwrap_or(CATALOG_PAGE_DEFAULT)
                .clamp(1, CATALOG_PAGE_MAX),
            filter: non_empty(self.filter),
        }
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Every node of one level, before filtering and paging.
///
/// `schema_tree` is the plugin's explorer layout: objects sit under schemas.
pub fn catalog_level(
    databases: &[CatalogDatabase],
    parent: &CatalogPath,
    schema_tree: bool,
) -> ConnectorResult<Vec<CatalogNode>> {
    if !parent.prefix.is_empty() {
        return Err(ConnectorError::NotSupported(
            "this engine has no nesting below a schema".into(),
        ));
    }
    let Some(db_name) = parent.database.as_deref() else {
        return Ok(databases
            .iter()
            .map(|db| CatalogNode {
                name: db.name.clone(),
                kind: KIND_DATABASE.into(),
                database: None,
                schema: None,
                has_children: true,
                row_count: None,
                meta: db.meta.clone(),
            })
            .collect());
    };
    let objects: &[CatalogObject] = databases
        .iter()
        .find(|db| db.name == db_name)
        .map(|db| db.objects.as_slice())
        .unwrap_or(&[]);
    if schema_tree && parent.schema.is_none() {
        let mut schemas: Vec<String> = objects.iter().filter_map(|o| o.schema.clone()).collect();
        schemas.sort();
        schemas.dedup();
        return Ok(schemas
            .into_iter()
            .map(|name| CatalogNode {
                name,
                kind: KIND_SCHEMA.into(),
                database: Some(db_name.to_string()),
                schema: None,
                has_children: true,
                row_count: None,
                meta: None,
            })
            .collect());
    }
    Ok(objects
        .iter()
        .filter(|o| match parent.schema.as_deref() {
            Some(s) => o.schema.as_deref() == Some(s),
            None => true,
        })
        .map(|o| CatalogNode {
            name: o.name.clone(),
            kind: o.kind.clone(),
            database: Some(o.database.clone().unwrap_or_else(|| db_name.to_string())),
            schema: o.schema.clone(),
            has_children: false,
            row_count: (o.row_count > 0).then_some(o.row_count),
            meta: None,
        })
        .collect())
}

/// Filter by name and cut one page. The cursor is the offset into the
/// filtered level, so it is only meaningful to the default implementation.
pub fn page_nodes(
    nodes: Vec<CatalogNode>,
    input: &ListCatalogChildrenInput,
) -> ConnectorResult<CatalogPage> {
    let start = match input.cursor.as_deref() {
        None => 0,
        Some(c) => c
            .strip_prefix("o:")
            .and_then(|n| n.parse::<usize>().ok())
            .ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("unknown catalog cursor `{c}`"))
            })?,
    };
    let needle = input.filter.as_deref().map(str::to_lowercase);
    let matching: Vec<CatalogNode> = nodes
        .into_iter()
        .filter(|n| match needle.as_deref() {
            Some(q) => n.name.to_lowercase().contains(q),
            None => true,
        })
        .collect();
    let limit = input.limit.max(1) as usize;
    let end = start.saturating_add(limit).min(matching.len());
    let next_cursor = (end < matching.len()).then(|| format!("o:{end}"));
    let nodes = matching
        .into_iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();
    Ok(CatalogPage { nodes, next_cursor })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(schema: Option<&str>, name: &str, kind: &str) -> CatalogObject {
        CatalogObject {
            name: name.into(),
            schema: schema.map(str::to_string),
            database: Some("app".into()),
            kind: kind.into(),
            row_count: 0,
        }
    }

    fn catalog() -> Vec<CatalogDatabase> {
        vec![
            CatalogDatabase {
                name: "app".into(),
                meta: Some("3 tables".into()),
                system: false,
                objects: vec![
                    obj(Some("public"), "users", "table"),
                    obj(Some("public"), "orders", "table"),
                    obj(Some("audit"), "log", "table"),
                    obj(Some("public"), "touch()", "function"),
                ],
            },
            CatalogDatabase {
                name: "other".into(),
                meta: None,
                system: false,
                objects: vec![],
            },
        ]
    }

    fn input(parent: CatalogPath, limit: u32) -> ListCatalogChildrenInput {
        ListCatalogChildrenInput {
            parent,
            cursor: None,
            limit,
            filter: None,
        }
    }

    #[test]
    fn the_root_lists_databases() {
        let level = catalog_level(&catalog(), &CatalogPath::default(), true).expect("level");
        let names: Vec<_> = level.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["app", "other"]);
        assert!(level
            .iter()
            .all(|n| n.kind == KIND_DATABASE && n.has_children));
    }

    #[test]
    fn a_schema_tree_lists_schemas_then_their_objects() {
        let db = CatalogPath {
            database: Some("app".into()),
            ..CatalogPath::default()
        };
        let schemas = catalog_level(&catalog(), &db, true).expect("schemas");
        let names: Vec<_> = schemas.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["audit", "public"]);

        let public = CatalogPath {
            schema: Some("public".into()),
            ..db.clone()
        };
        let objects = catalog_level(&catalog(), &public, true).expect("objects");
        let names: Vec<_> = objects.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["users", "orders", "touch()"]);
        assert!(objects.iter().all(|n| !n.has_children));

        // A flat layout skips the schema level.
        let flat = catalog_level(&catalog(), &db, false).expect("flat");
        assert_eq!(flat.len(), 4);
    }

    #[test]
    fn pages_follow_the_cursor_to_the_end() {
        let db = CatalogPath {
            database: Some("app".into()),
            ..CatalogPath::default()
        };
        let all = catalog_level(&catalog(), &db, false).expect("level");
        let mut req = input(db, 3);
        let first = page_nodes(all.clone(), &req).expect("page 1");
        assert_eq!(first.nodes.len(), 3);
        assert_eq!(first.next_cursor.as_deref(), Some("o:3"));
        req.cursor = first.next_cursor;
        let second = page_nodes(all, &req).expect("page 2");
        assert_eq!(second.nodes.len(), 1);
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn the_filter_runs_before_paging() {
        let db = CatalogPath {
            database: Some("app".into()),
            ..CatalogPath::default()
        };
        let all = catalog_level(&catalog(), &db, false).expect("level");
        let mut req = input(db, 1);
        req.filter = Some("ORD".into());
        let page = page_nodes(all, &req).expect("page");
        assert_eq!(page.nodes.len(), 1);
        assert_eq!(page.nodes[0].name, "orders");
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn a_foreign_cursor_is_refused() {
        let mut req = input(CatalogPath::default(), 10);
        req.cursor = Some("scan:42".into());
        assert!(page_nodes(Vec::new(), &req).is_err());
    }

    #[test]
    fn the_request_trims_and_clamps() {
        let input = ListCatalogChildrenRequest {
            session_id: "s".into(),
            parent: CatalogPath {
                database: Some("  ".into()),
                schema: Some(" public ".into()),
                prefix: vec![],
            },
            cursor: Some(String::new()),
            limit: Some(1_000_000),
            filter: Some("  ".into()),
        }
        .into_input();
        assert_eq!(input.parent.database, None);
        assert_eq!(input.parent.schema.as_deref(), Some("public"));
        assert_eq!(input.cursor, None);
        assert_eq!(input.limit, CATALOG_PAGE_MAX);
        assert_eq!(input.filter, None);
    }

    #[test]
    fn nesting_below_a_schema_needs_a_native_plugin() {
        let path = CatalogPath {
            database: Some("app".into()),
            schema: None,
            prefix: vec!["user".into()],
        };
        assert!(catalog_level(&catalog(), &path, false).is_err());
    }
}
