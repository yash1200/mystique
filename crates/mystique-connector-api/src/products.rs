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
    /// Home picker card. `false` = resolve-only until Phase 1.4.
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
        on_picker: false,
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
        on_picker: false,
        support_depth: crate::SupportDepth::Operate,
    },
    EngineProduct {
        id: "tidb",
        runtime: "mysql",
        display_name: "TiDB",
        mark: "Ti",
        default_port: 4000,
        description: "TiDB — MySQL wire protocol.",
        on_picker: false,
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
        on_picker: false,
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
    fn picker_is_four_runtimes_until_phase_1_4() {
        let cards: Vec<_> = ENGINE_PRODUCTS
            .iter()
            .filter(|p| p.on_picker)
            .map(|p| p.id)
            .collect();
        assert_eq!(cards, ["postgres", "mysql", "sqlite", "mongodb"]);
    }

    #[test]
    fn products_default_to_operate() {
        assert!(ENGINE_PRODUCTS
            .iter()
            .all(|p| p.support_depth == crate::SupportDepth::Operate));
    }
}
