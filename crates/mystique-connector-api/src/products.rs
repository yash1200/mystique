//! Product vs runtime.
//!
//! A **product** is what the user picks and what we store on the connection
//! (`cockroachdb`). A **runtime** is the plugin crate that opens the pool
//! (`postgres`). Keep this table in sync with `src/lib/engines/products.ts`.

/// One named database in the picker / saved-profile `engine` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineProduct {
    pub id: &'static str,
    pub runtime: &'static str,
    pub display_name: &'static str,
    pub mark: &'static str,
    pub default_port: u16,
    pub description: &'static str,
    /// Home picker card. Aliases stay `false` (resolve-only).
    pub on_picker: bool,
    pub support_depth: crate::SupportDepth,
}

/// Identity runtimes plus products that share a wire protocol.
pub const ENGINE_PRODUCTS: &[EngineProduct] = &[
    EngineProduct {
        id: "postgres",
        runtime: "postgres",
        display_name: "PostgreSQL",
        mark: "Pg",
        default_port: 5432,
        description:
            "Relational studio — schemas, tables, SQL, and classic ER Model view from foreign keys.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "postgresql",
        runtime: "postgres",
        display_name: "PostgreSQL",
        mark: "Pg",
        default_port: 5432,
        description: "PostgreSQL (postgresql alias).",
        on_picker: false,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "pg",
        runtime: "postgres",
        display_name: "PostgreSQL",
        mark: "Pg",
        default_port: 5432,
        description: "PostgreSQL (pg alias).",
        on_picker: false,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "cockroachdb",
        runtime: "postgres",
        display_name: "CockroachDB",
        mark: "CR",
        default_port: 26257,
        description: "Distributed SQL — Postgres wire protocol.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "mysql",
        runtime: "mysql",
        display_name: "MySQL",
        mark: "My",
        default_port: 3306,
        description: "MySQL — databases, tables, SQL query space, and relational Model view.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "mariadb",
        runtime: "mysql",
        display_name: "MariaDB",
        mark: "Ma",
        default_port: 3306,
        description: "MariaDB — MySQL wire protocol.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "tidb",
        runtime: "mysql",
        display_name: "TiDB",
        mark: "Ti",
        default_port: 4000,
        description: "TiDB — MySQL wire protocol.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "sqlite",
        runtime: "sqlite",
        display_name: "SQLite",
        mark: "Lite",
        default_port: 0,
        description: "Local file database.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "mongodb",
        runtime: "mongodb",
        display_name: "MongoDB",
        mark: "Mo",
        default_port: 27017,
        description:
            "Document studio — databases, collections, find/aggregation, and collection Model map.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "mongo",
        runtime: "mongodb",
        display_name: "MongoDB",
        mark: "Mo",
        default_port: 27017,
        description: "MongoDB (mongo alias).",
        on_picker: false,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "documentdb",
        runtime: "mongodb",
        display_name: "DocumentDB",
        mark: "Doc",
        default_port: 27017,
        description: "Amazon DocumentDB — MongoDB wire protocol.",
        on_picker: true,
        support_depth: crate::SupportDepth::Operate,
    },
];

fn norm(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

pub fn product(id: &str) -> Option<&'static EngineProduct> {
    let key = norm(id);
    ENGINE_PRODUCTS.iter().find(|p| p.id == key)
}

/// Runtime crate id for a product (or alias). `None` if unknown.
pub fn runtime_id(id: &str) -> Option<&'static str> {
    product(id).map(|p| p.runtime)
}

/// DocumentDB rejects the Mongo driver's default `retryWrites=true`.
/// `Some(false)` forces the driver flag off. `None` leaves the driver default.
pub fn retry_writes_override(product_id: &str) -> Option<bool> {
    match product(product_id).map(|p| p.id) {
        Some("documentdb") => Some(false),
        _ => None,
    }
}

/// Stamp a connect-time overlay onto a copy of `plugin_config`.
/// Saved profiles are not rewritten; the user does not edit the URI.
pub fn stamp_connect_overlay(product_id: &str, plugin_config: &mut serde_json::Value) {
    let Some(flag) = retry_writes_override(product_id) else {
        return;
    };
    let Some(obj) = plugin_config.as_object_mut() else {
        return;
    };
    obj.insert("retryWrites".into(), serde_json::Value::Bool(flag));
}

/// Cockroach has no `pg_terminate_backend`. Hide terminate until CANCEL QUERY exists.
pub fn forces_terminate_off(product_id: &str) -> bool {
    product(product_id).is_some_and(|p| p.id == "cockroachdb")
}

/// Schemas, or MySQL/TiDB databases, that are not user data.
/// Compared case-insensitively so TiDB's `INFORMATION_SCHEMA` matches.
pub fn hidden_schemas(product_id: &str) -> &'static [&'static str] {
    match product(product_id).map(|p| p.id) {
        Some("cockroachdb") => &["crdb_internal", "pg_extension"],
        Some("tidb") => &[
            "information_schema",
            "performance_schema",
            "metrics_schema",
            "inspection_schema",
            "mysql",
            "sys",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_runtimes_map_to_themselves() {
        assert_eq!(runtime_id("postgres"), Some("postgres"));
        assert_eq!(runtime_id("MYSQL"), Some("mysql"));
        assert_eq!(runtime_id(" sqlite "), Some("sqlite"));
        assert_eq!(runtime_id("mongodb"), Some("mongodb"));
    }

    #[test]
    fn products_share_wire_runtimes() {
        assert_eq!(runtime_id("mariadb"), Some("mysql"));
        assert_eq!(runtime_id("tidb"), Some("mysql"));
        assert_eq!(runtime_id("cockroachdb"), Some("postgres"));
        assert_eq!(runtime_id("documentdb"), Some("mongodb"));
        assert_eq!(runtime_id("postgresql"), Some("postgres"));
        assert_eq!(runtime_id("pg"), Some("postgres"));
        assert_eq!(runtime_id("mongo"), Some("mongodb"));
    }

    #[test]
    fn unknown_product_is_none() {
        assert!(runtime_id("oracle").is_none());
        assert!(runtime_id("").is_none());
    }

    #[test]
    fn picker_includes_first_class_profiles() {
        let cards: Vec<_> = ENGINE_PRODUCTS
            .iter()
            .filter(|p| p.on_picker)
            .map(|p| p.id)
            .collect();
        assert_eq!(
            cards,
            [
                "postgres",
                "cockroachdb",
                "mysql",
                "mariadb",
                "tidb",
                "sqlite",
                "mongodb",
                "documentdb",
            ]
        );
    }

    #[test]
    fn aliases_are_not_picker_cards() {
        for id in ["postgresql", "pg", "mongo"] {
            assert!(!product(id).unwrap().on_picker);
        }
    }

    #[test]
    fn picker_products_reuse_four_runtimes() {
        let mut runtimes: Vec<_> = ENGINE_PRODUCTS
            .iter()
            .filter(|p| p.on_picker)
            .map(|p| p.runtime)
            .collect();
        runtimes.sort_unstable();
        runtimes.dedup();
        assert_eq!(runtimes, ["mongodb", "mysql", "postgres", "sqlite"]);
    }

    #[test]
    fn products_default_to_operate() {
        assert!(ENGINE_PRODUCTS
            .iter()
            .all(|p| p.support_depth == crate::SupportDepth::Operate));
    }

    #[test]
    fn documentdb_disables_retry_writes_and_mongodb_does_not() {
        assert_eq!(retry_writes_override("documentdb"), Some(false));
        assert_eq!(retry_writes_override("mongodb"), None);
        assert_eq!(retry_writes_override("mongo"), None);
        let mut config =
            serde_json::json!({"uri": "mongodb://docdb.internal:27017/app?retryWrites=true"});
        stamp_connect_overlay("documentdb", &mut config);
        assert_eq!(config["retryWrites"], false);
        let mut mongo = serde_json::json!({"uri": "mongodb://127.0.0.1/app"});
        stamp_connect_overlay("mongodb", &mut mongo);
        assert!(mongo.get("retryWrites").is_none());
    }

    #[test]
    fn cockroach_hides_terminate_and_internal_schemas() {
        assert!(forces_terminate_off("cockroachdb"));
        assert!(!forces_terminate_off("postgres"));
        assert_eq!(
            hidden_schemas("cockroachdb"),
            ["crdb_internal", "pg_extension"]
        );
        assert!(hidden_schemas("postgres").is_empty());
    }

    #[test]
    fn tidb_hides_system_schemas() {
        assert_eq!(
            hidden_schemas("tidb"),
            [
                "information_schema",
                "performance_schema",
                "metrics_schema",
                "inspection_schema",
                "mysql",
                "sys",
            ]
        );
        assert!(hidden_schemas("mysql").is_empty());
    }
}
