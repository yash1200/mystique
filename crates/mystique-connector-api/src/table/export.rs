//! Table / collection file export.
//!
//! The plugin streams rows ([`crate::ConnectorSession::stream_rows`]) into an
//! [`ExportWriter`] sink. The sink hands each batch to a writer thread over a
//! bounded channel, so memory stays at a few batches whatever the table size.
//! Rows go to a staged file next to the destination; the destination is
//! replaced by an atomic rename only when every row was written. A failed or
//! cancelled export leaves the destination as it was.

use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot};

use crate::cancel::QueryControl;
use crate::catalog::{ColumnInfo, FetchTablePreviewInput};
use crate::connector::ConnectorSession;
use crate::error::{ConnectorError, ConnectorResult};
use crate::stream::{stream_cancelled, RowSink, RowSource, STREAM_BATCH_ROWS};

use mystique_sql::SqlDialect;
use mystique_transfer::export::{
    write_csv_header, write_csv_rows, write_io, write_json_array_rows, write_jsonl_rows,
    write_sql_insert, writeln_io,
};

/// shim: remove after one release. Defined in `mystique-transfer`.
pub use mystique_transfer::ExportFormat;

/// Rows per page when a plugin has no streaming read and pages by offset.
pub const EXPORT_CHUNK_SIZE: u32 = STREAM_BATCH_ROWS as u32;

/// Batches the writer thread may have queued. Bounds export memory.
const WRITER_QUEUE: usize = 4;
/// Rows per `INSERT` statement in a SQL export.
const SQL_INSERT_ROWS: usize = 100;
/// Progress callbacks at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// IPC request. `path` is the absolute destination file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ExportObjectRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub format: ExportFormat,
    pub path: String,
    /// Cancel token for this export. The transfer queue owns it.
    #[serde(default)]
    pub execution_id: Option<String>,
    /// A query tab whose open transaction the export reads through, so rows
    /// it changed but has not committed are exported as the tab sees them.
    #[serde(default)]
    pub tab_id: Option<String>,
}

/// Rows exported or imported so far, sent over IPC while a transfer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub rows: u64,
}

/// Rows written so far. Called from the writer thread.
#[derive(Clone)]
pub struct RowProgress(pub Arc<dyn Fn(u64) + Send + Sync>);

impl fmt::Debug for RowProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RowProgress")
    }
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct ExportObjectInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub format: ExportFormat,
    pub path: String,
    pub control: Arc<QueryControl>,
    pub progress: Option<RowProgress>,
}

impl ExportObjectRequest {
    pub fn into_input(self) -> ExportObjectInput {
        ExportObjectInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            format: self.format,
            path: self.path,
            control: QueryControl::new(),
            progress: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ExportObjectResult {
    pub path: String,
    pub row_count: u64,
    pub format: ExportFormat,
}

impl ExportObjectInput {
    pub fn source(&self) -> RowSource {
        RowSource::Table {
            database: self.database.clone(),
            schema: self.schema.clone(),
            table: self.name.clone(),
        }
    }
}

/// Plugins without a streaming read: page through the table preview.
/// shim: remove after one release (the Mongo plugin still calls it).
pub async fn export_object_with_preview(
    session: &dyn ConnectorSession,
    input: &ExportObjectInput,
    sql_style: Option<SqlDialect>,
) -> ConnectorResult<ExportObjectResult> {
    let mut writer = ExportWriter::start(input, sql_style)?;
    let outcome = session
        .stream_rows(&input.source(), &input.control, writer.sink())
        .await;
    writer.finish(outcome).await
}

/// Offset paging over [`ConnectorSession::fetch_table_preview`], one
/// [`EXPORT_CHUNK_SIZE`] page at a time. The default `stream_rows`.
pub async fn stream_table_by_preview<S: ConnectorSession + ?Sized>(
    session: &S,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    control: &Arc<QueryControl>,
    sink: &mut dyn RowSink,
) -> ConnectorResult<()> {
    let mut offset: u32 = 0;
    let mut sent_columns = false;
    loop {
        stream_cancelled(control)?;
        let page = session
            .fetch_table_preview(&FetchTablePreviewInput {
                table: table.to_string(),
                schema: schema.map(str::to_string),
                database: database.map(str::to_string),
                max_rows: EXPORT_CHUNK_SIZE,
                offset,
                filters: Vec::new(),
                code_filter: None,
                sort_column: None,
                sort_dir: None,
                control: Arc::clone(control),
            })
            .await?;
        if !sent_columns && !page.columns.is_empty() {
            sink.columns(page.columns).await?;
            sent_columns = true;
        }
        let count = page.rows.len();
        if count > 0 {
            sink.rows(page.rows).await?;
        }
        if count < EXPORT_CHUNK_SIZE as usize {
            return Ok(());
        }
        let Some(next) = offset.checked_add(EXPORT_CHUNK_SIZE) else {
            return Ok(());
        };
        offset = next;
    }
}

enum WriterMsg {
    Columns(Vec<ColumnInfo>),
    Rows(Vec<Vec<JsonValue>>),
}

/// The sink a plugin streams into. Sending blocks (asynchronously) while the
/// writer is [`WRITER_QUEUE`] batches behind.
pub struct ExportSink {
    tx: Option<mpsc::Sender<WriterMsg>>,
}

#[async_trait]
impl RowSink for ExportSink {
    async fn columns(&mut self, columns: Vec<ColumnInfo>) -> ConnectorResult<()> {
        self.send(WriterMsg::Columns(columns)).await
    }

    async fn rows(&mut self, rows: Vec<Vec<JsonValue>>) -> ConnectorResult<()> {
        self.send(WriterMsg::Rows(rows)).await
    }
}

impl ExportSink {
    async fn send(&mut self, msg: WriterMsg) -> ConnectorResult<()> {
        let Some(tx) = self.tx.as_ref() else {
            return Err(writer_gone());
        };
        tx.send(msg).await.map_err(|_| writer_gone())
    }
}

fn writer_gone() -> ConnectorError {
    // The writer thread stopped; `finish` reports its own error.
    ConnectorError::Internal("export writer stopped".into())
}

/// A running file export: a writer thread and the sink feeding it.
pub struct ExportWriter {
    sink: ExportSink,
    done: oneshot::Receiver<ConnectorResult<u64>>,
    /// Set by `finish(Ok)`. Without it the writer discards the staged file.
    commit: Arc<std::sync::atomic::AtomicBool>,
    path: String,
    format: ExportFormat,
}

impl ExportWriter {
    /// Validate, create the staged file and start the writer thread.
    pub fn start(
        input: &ExportObjectInput,
        sql_style: Option<SqlDialect>,
    ) -> ConnectorResult<Self> {
        if matches!(input.format, ExportFormat::Sql) && sql_style.is_none() {
            return Err(ConnectorError::NotSupported(
                "SQL export is not available for this engine.".into(),
            ));
        }
        if input.path.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Export path is required.".into(),
            ));
        }
        if input.name.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        let dest = PathBuf::from(&input.path);
        let staged = staged_path(&dest);
        let file = File::create(&staged).map_err(|e| {
            ConnectorError::Internal(format!("could not create {}: {e}", staged.display()))
        })?;
        let (tx, rx) = mpsc::channel(WRITER_QUEUE);
        let (done_tx, done_rx) = oneshot::channel();
        let commit = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let job = WriterJob {
            out: BufWriter::with_capacity(256 * 1024, file),
            staged,
            dest,
            format: input.format,
            sql_style,
            database: input.database.clone(),
            schema: input.schema.clone(),
            table: input.name.clone(),
            progress: input.progress.clone(),
            commit: Arc::clone(&commit),
        };
        let spawned = std::thread::Builder::new()
            .name("mystique-export".into())
            .spawn(move || {
                let _ = done_tx.send(job.run(rx));
            });
        if let Err(err) = spawned {
            return Err(ConnectorError::Internal(format!(
                "could not start the export writer: {err}"
            )));
        }
        Ok(Self {
            sink: ExportSink { tx: Some(tx) },
            done: done_rx,
            commit,
            path: input.path.clone(),
            format: input.format,
        })
    }

    pub fn sink(&mut self) -> &mut dyn RowSink {
        &mut self.sink
    }

    /// End the export. `outcome` is the read's result: on success the staged
    /// file replaces the destination; on failure it is deleted. A writer
    /// error wins over the read error it caused.
    pub async fn finish(
        mut self,
        outcome: ConnectorResult<()>,
    ) -> ConnectorResult<ExportObjectResult> {
        if outcome.is_ok() {
            self.commit.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        // Closing the channel tells the writer the read is over.
        self.sink.tx = None;
        let written = match self.done.await {
            Ok(result) => result,
            Err(_) => Err(ConnectorError::Internal(
                "export writer stopped without a result".into(),
            )),
        };
        match (outcome, written) {
            (Ok(()), Ok(row_count)) => Ok(ExportObjectResult {
                path: self.path,
                row_count,
                format: self.format,
            }),
            (_, Err(write_err)) if !matches!(write_err, ConnectorError::Query(_)) => Err(write_err),
            (Err(read_err), _) => Err(read_err),
            (Ok(()), Err(write_err)) => Err(write_err),
        }
    }
}

/// `report.csv` → `.report.csv.mystique-part` in the same directory, so the
/// final rename stays on one file system.
fn staged_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    dest.with_file_name(format!(".{name}.mystique-part"))
}

struct WriterJob {
    out: BufWriter<File>,
    staged: PathBuf,
    dest: PathBuf,
    format: ExportFormat,
    sql_style: Option<SqlDialect>,
    database: Option<String>,
    schema: Option<String>,
    table: String,
    progress: Option<RowProgress>,
    commit: Arc<std::sync::atomic::AtomicBool>,
}

impl WriterJob {
    fn run(mut self, rx: mpsc::Receiver<WriterMsg>) -> ConnectorResult<u64> {
        let written = self.write_all(rx);
        let committed = self.commit.load(std::sync::atomic::Ordering::SeqCst);
        let result = match written {
            Ok(rows) if committed => self.publish().map(|()| rows),
            Ok(_) => Err(ConnectorError::Query("Export stopped.".into())),
            Err(err) => Err(err),
        };
        if result.is_err() {
            let _ = std::fs::remove_file(&self.staged);
        }
        result
    }

    fn write_all(&mut self, mut rx: mpsc::Receiver<WriterMsg>) -> ConnectorResult<u64> {
        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut rows_written: u64 = 0;
        let mut json_started = false;
        let mut last_report = Instant::now();
        if matches!(self.format, ExportFormat::Json) {
            write_io(&mut self.out, b"[\n")?;
        }
        while let Some(msg) = rx.blocking_recv() {
            match msg {
                WriterMsg::Columns(cols) => {
                    if columns.is_empty() {
                        columns = cols;
                        if matches!(self.format, ExportFormat::Csv) {
                            write_csv_header(&mut self.out, &columns)?;
                        }
                    }
                }
                WriterMsg::Rows(rows) => {
                    self.write_rows(&columns, &rows, json_started)?;
                    json_started = json_started || !rows.is_empty();
                    rows_written += rows.len() as u64;
                    if last_report.elapsed() >= PROGRESS_EVERY {
                        last_report = Instant::now();
                        if let Some(progress) = &self.progress {
                            (progress.0)(rows_written);
                        }
                    }
                }
            }
        }
        match self.format {
            ExportFormat::Json => write_io(&mut self.out, b"]\n")?,
            ExportFormat::Csv if columns.is_empty() => write_io(&mut self.out, b"\n")?,
            ExportFormat::Sql if rows_written == 0 => {
                writeln_io(
                    &mut self.out,
                    &format!("-- No rows to export for {}\n", self.table),
                )?;
            }
            _ => {}
        }
        self.out.flush().map_err(|e| {
            ConnectorError::Internal(format!("could not write {}: {e}", self.dest.display()))
        })?;
        self.out.get_ref().sync_all().map_err(|e| {
            ConnectorError::Internal(format!("could not write {}: {e}", self.dest.display()))
        })?;
        if let Some(progress) = &self.progress {
            (progress.0)(rows_written);
        }
        Ok(rows_written)
    }

    fn write_rows(
        &mut self,
        columns: &[ColumnInfo],
        rows: &[Vec<JsonValue>],
        json_started: bool,
    ) -> ConnectorResult<()> {
        match self.format {
            ExportFormat::Csv => write_csv_rows(&mut self.out, columns, rows)?,
            ExportFormat::Json => {
                write_json_array_rows(&mut self.out, columns, rows, json_started)?
            }
            ExportFormat::Jsonl => write_jsonl_rows(&mut self.out, columns, rows)?,
            ExportFormat::Sql => {
                let Some(style) = self.sql_style else {
                    return Err(ConnectorError::NotSupported(
                        "SQL export is not available for this engine.".into(),
                    ));
                };
                for chunk in rows.chunks(SQL_INSERT_ROWS) {
                    write_sql_insert(
                        &mut self.out,
                        style,
                        self.database.as_deref(),
                        self.schema.as_deref(),
                        &self.table,
                        columns,
                        chunk,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn publish(&self) -> ConnectorResult<()> {
        std::fs::rename(&self.staged, &self.dest).map_err(|e| {
            ConnectorError::Internal(format!("could not write {}: {e}", self.dest.display()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "mystique-export-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn input(path: &Path, format: ExportFormat) -> ExportObjectInput {
        ExportObjectInput {
            name: "t".into(),
            schema: None,
            database: None,
            kind: None,
            format,
            path: path.to_string_lossy().into_owned(),
            control: QueryControl::new(),
            progress: None,
        }
    }

    fn column(name: &str) -> ColumnInfo {
        crate::stream::stream_column(name, "text")
    }

    #[tokio::test]
    async fn rows_land_in_the_destination_only_on_success() {
        let dir = temp_dir("ok");
        let dest = dir.join("out.jsonl");
        std::fs::write(&dest, "previous").expect("seed");
        let mut writer =
            ExportWriter::start(&input(&dest, ExportFormat::Jsonl), None).expect("start");
        writer
            .sink()
            .columns(vec![column("id"), column("name")])
            .await
            .expect("columns");
        for batch in 0..3 {
            let rows = (0..STREAM_BATCH_ROWS)
                .map(|i| vec![json!(batch * STREAM_BATCH_ROWS + i), json!("x")])
                .collect();
            writer.sink().rows(rows).await.expect("rows");
            // The destination is untouched while rows stream.
            assert_eq!(std::fs::read_to_string(&dest).expect("read"), "previous");
        }
        let done = writer.finish(Ok(())).await.expect("finish");
        assert_eq!(done.row_count, 3 * STREAM_BATCH_ROWS as u64);
        let text = std::fs::read_to_string(&dest).expect("read");
        assert_eq!(text.lines().count(), 3 * STREAM_BATCH_ROWS);
        assert!(!staged_path(&dest).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_failed_read_keeps_the_old_file_and_removes_the_staged_one() {
        let dir = temp_dir("fail");
        let dest = dir.join("out.csv");
        std::fs::write(&dest, "previous").expect("seed");
        let mut writer =
            ExportWriter::start(&input(&dest, ExportFormat::Csv), None).expect("start");
        writer
            .sink()
            .columns(vec![column("id")])
            .await
            .expect("cols");
        writer
            .sink()
            .rows(vec![vec![json!(1)]])
            .await
            .expect("rows");
        let err = writer
            .finish(Err(ConnectorError::Query("Query cancelled.".into())))
            .await
            .expect_err("cancelled");
        assert!(err.to_string().contains("cancelled"), "{err}");
        assert_eq!(std::fs::read_to_string(&dest).expect("read"), "previous");
        assert!(!staged_path(&dest).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn json_and_sql_exports_are_complete_documents() {
        let dir = temp_dir("json");
        let dest = dir.join("out.json");
        let mut writer =
            ExportWriter::start(&input(&dest, ExportFormat::Json), None).expect("start");
        writer
            .sink()
            .columns(vec![column("id")])
            .await
            .expect("cols");
        writer
            .sink()
            .rows(vec![vec![json!(1)]])
            .await
            .expect("rows");
        writer
            .sink()
            .rows(vec![vec![json!(2)]])
            .await
            .expect("rows");
        writer.finish(Ok(())).await.expect("finish");
        let parsed: JsonValue =
            serde_json::from_str(&std::fs::read_to_string(&dest).expect("read")).expect("json");
        assert_eq!(parsed, json!([{ "id": 1 }, { "id": 2 }]));

        let dest = dir.join("out.sql");
        let mut writer =
            ExportWriter::start(&input(&dest, ExportFormat::Sql), Some(SqlDialect::Postgres))
                .expect("start");
        writer
            .sink()
            .columns(vec![column("id")])
            .await
            .expect("cols");
        let rows = (0..250).map(|i| vec![json!(i.to_string())]).collect();
        writer.sink().rows(rows).await.expect("rows");
        writer.finish(Ok(())).await.expect("finish");
        let sql = std::fs::read_to_string(&dest).expect("read");
        assert_eq!(sql.matches("INSERT INTO").count(), 3, "{sql}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
