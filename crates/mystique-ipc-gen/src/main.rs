//! `pnpm gen:ipc`: write `src/lib/core/ipc/bindings.gen.ts`.
//!
//! Every Tauri command argument and return DTO is listed in [`roots`]; the
//! types they reference are found by walking ts-rs dependencies. The output
//! is one file, sorted by type name, so a diff shows exactly what changed.
//! CI regenerates it and fails when the checked-in file differs.
//!
//! Usage: `cargo run -p mystique-ipc-gen --features ts --locked [-- <out>]`

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use ts_rs::{Config, TypeVisitor, TS};

const HEADER: &str = "\
// generated, do not edit.
// Source: the Rust IPC types (ts-rs). Regenerate with `pnpm gen:ipc`.
/* eslint-disable */
";

/// Types the TS side treats as opaque JSON. ts-rs would emit a recursive
/// `JsonValue` union; plugin config and cell values are read with checks.
const OPAQUE: &[(&str, &str)] = &[("JsonValue", "unknown")];

struct Collect {
    cfg: Config,
    seen: HashSet<TypeId>,
    /// TS name → (Rust type, declaration).
    decls: BTreeMap<String, (&'static str, String)>,
}

impl TypeVisitor for Collect {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        if T::output_path().is_some() {
            let name = T::ident(&self.cfg);
            let rust = std::any::type_name::<T>();
            if let Some((ts, ty)) = OPAQUE.iter().find(|(ts, _)| *ts == name) {
                // ts-rs has more than one Rust type behind `JsonValue`.
                self.decls
                    .insert(name, ("<opaque>", format!("export type {ts} = {ty};")));
                return;
            }
            let docs = T::docs().unwrap_or_default();
            let decl = format!("{docs}export {}", T::decl(&self.cfg));
            if let Some((other, _)) = self.decls.get(&name) {
                if *other != rust {
                    panic!(
                        "two Rust types export as `{name}`: {other} and {rust}; add #[ts(rename)]"
                    );
                }
            }
            self.decls.insert(name, (rust, decl));
        }
        T::visit_dependencies(self);
        T::visit_generics(self);
    }
}

macro_rules! roots {
    ($c:expr; $($ty:ty),* $(,)?) => {{
        $( $c.visit::<$ty>(); )*
    }};
}

/// Arguments and results of every command in `src-tauri/src/lib.rs`, plus
/// the error envelope and the batch progress event.
fn roots(c: &mut Collect) {
    use mystique_acp as acp;
    use mystique_agent as agent;
    use mystique_core as core;
    use mystique_mcp as mcp;
    roots!(c;
        // Error envelope and channel events.
        core::IpcError,
        core::BatchEvent,
        // Engines and sessions (ipc/engines.rs).
        core::ParseConnectionUrlRequest,
        core::ParsedConnectionUrl,
        core::FormatConnectionUrlRequest,
        core::FormattedConnectionUrl,
        core::SshHostKeyPrompt,
        core::OpenSessionRequest,
        core::TestResult,
        core::SessionInfo,
        core::PinStatus,
        core::ExecuteQueryRequest,
        core::QueryBatchResult,
        core::ExplainQueryRequest,
        core::ExplainPlan,
        core::DescribeTableRequest,
        core::ColumnInfo,
        core::ListCatalogColumnsRequest,
        core::CatalogColumnRow,
        core::ApplyTableEditsRequest,
        core::ApplyTableEditsResult,
        core::DistinctColumnValuesRequest,
        core::RollbackTableEditsRequest,
        core::FetchTablePreviewRequest,
        core::TablePreviewResult,
        core::FetchLargeValueRequest,
        core::FetchLargeValueResult,
        core::FetchTablePreviewStatsRequest,
        core::TablePreviewStats,
        core::TablePreviewCount,
        core::TableCountOutcome,
        core::ListCatalogRequest,
        core::CatalogDatabase,
        core::ListCatalogChildrenRequest,
        core::CatalogPath,
        core::CatalogPage,
        core::CatalogNode,
        core::ListRelationshipsRequest,
        core::CatalogRelationship,
        core::PreviewImportRequest,
        core::PreviewImportResult,
        core::ImportObjectRequest,
        core::ImportObjectResult,
        core::ExportObjectRequest,
        core::ExportObjectResult,
        core::TransferProgress,
        core::TableAdminRequest,
        core::TableAdminResult,
        core::DescribeTableDefinitionRequest,
        core::TableDefinition,
        core::ListTableDefinitionsRequest,
        core::PlanDdlRequest,
        core::DdlPlan,
        core::ApplyDdlRequest,
        core::ApplyDdlResult,
        core::PlanSchemaDiffRequest,
        core::SchemaMigration,
        core::ApplySchemaDiffRequest,
        core::ObjectStructureRequest,
        core::ObjectStructure,
        core::FetchServerMetricsRequest,
        core::ServerMetricsSnapshot,
        core::TerminateSessionRequest,
        core::TerminateSessionResult,
        core::EngineCapabilities,
        // Write safety: timed unlock and the affected-row preview.
        core::TimedUnlockStatus,
        core::PreviewWriteRowsRequest,
        core::WriteRowPreview,
        // Profiles, settings, vault.
        core::ConnectionProfile,
        core::SaveConnectionInput,
        core::VaultStatus,
        core::PurgeReport,
        // History, saved queries, tabs, grid layout.
        core::AppendHistoryInput,
        core::QueryHistoryItem,
        core::QueryHistoryPage,
        core::SaveQueryInput,
        core::SavedQuery,
        core::WorkspaceTabRecord,
        core::ReplaceWorkspaceTabsInput,
        core::SaveWorkspaceTabInput,
        core::GridColumnLayoutRecord,
        core::GridSaveSummary,
        // Agent.
        core::AgentChat,
        core::AgentMessage,
        core::UpdateAgentChatInput,
        agent::AiSettingsView,
        agent::AiKeyStatus,
        agent::AgentModelInfo,
        agent::SendAgentMessage,
        agent::SendAgentResult,
        agent::AgentEvent,
        // External (ACP) agent.
        acp::AcpSettingsView,
        acp::SaveAcpAgent,
        acp::AcpAgentSaved,
        acp::DetectedAcpAgent,
        acp::AcpProbe,
        // MCP.
        mcp::McpStatus,
        mcp::McpHarness,
        // MCP and agent access levels (ipc/mcp.rs).
        core::ToolAccessView,
        core::ToolApprovalRequest,
    );
}

fn render() -> String {
    // u64 / i64 travel as JSON numbers; serde_json writes them that way.
    let cfg = Config::default().with_large_int("number");
    let mut c = Collect {
        cfg,
        seen: HashSet::new(),
        decls: BTreeMap::new(),
    };
    roots(&mut c);
    let mut out = String::from(HEADER);
    for (_, (_, decl)) in c.decls {
        out.push('\n');
        out.push_str(decl.trim_end());
        out.push('\n');
    }
    out
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let repo = crate_dir.ancestors().nth(2).unwrap_or(crate_dir);
            repo.join("src/lib/core/ipc/bindings.gen.ts")
        });
    let text = render();
    if let Err(err) = std::fs::write(&out, &text) {
        eprintln!("could not write {}: {err}", out.display());
        std::process::exit(1);
    }
    println!("wrote {} ({} bytes)", out.display(), text.len());
}
