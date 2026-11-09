//! Row streaming for export and internal reads.
//!
//! A plugin pushes rows into a [`RowSink`] in batches of at most
//! [`STREAM_BATCH_ROWS`] as the driver returns them. A file export's sink
//! hands each batch to a writer thread over a bounded channel, so at most a
//! few batches are in memory however big the table is.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::cancel::QueryControl;
use crate::catalog::ColumnInfo;
use crate::error::{ConnectorError, ConnectorResult};

/// Rows per batch pushed into a sink.
pub const STREAM_BATCH_ROWS: usize = 1_000;

/// What to read.
#[derive(Debug, Clone)]
pub enum RowSource {
    /// Every row of a table or collection, in the engine's natural key order
    /// where it has one. The plugin picks the select list so values encode
    /// exactly as in the table grid.
    Table {
        database: Option<String>,
        schema: Option<String>,
        table: String,
    },
    /// One read-only statement with bound parameters (never interpolated).
    /// Internal use: the SQL is built by core, not typed by the user.
    Select { sql: String, params: Vec<JsonValue> },
}

/// Receives the columns once, then row batches. An error from the sink stops
/// the read (the writer failed, or the caller has enough rows).
#[async_trait]
pub trait RowSink: Send {
    async fn columns(&mut self, columns: Vec<ColumnInfo>) -> ConnectorResult<()>;
    async fn rows(&mut self, rows: Vec<Vec<JsonValue>>) -> ConnectorResult<()>;
}

/// Shared stop check for plugin read loops.
pub fn stream_cancelled(control: &QueryControl) -> ConnectorResult<()> {
    if control.is_cancelled() {
        Err(ConnectorError::Query("Query cancelled.".into()))
    } else {
        Ok(())
    }
}

/// A result column from a driver, as the writers need it.
pub fn stream_column(name: impl Into<String>, data_type: impl Into<String>) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        pk: false,
        enum_values: None,
        default_value: None,
        references: None,
    }
}

/// Keeps every row in memory, up to `limit`. Pushing past the limit is an
/// error so the plugin stops reading. For single-row internal reads.
pub struct CollectSink {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<JsonValue>>,
    limit: usize,
}

impl CollectSink {
    pub fn new(limit: usize) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            limit,
        }
    }
}

#[async_trait]
impl RowSink for CollectSink {
    async fn columns(&mut self, columns: Vec<ColumnInfo>) -> ConnectorResult<()> {
        self.columns = columns;
        Ok(())
    }

    async fn rows(&mut self, rows: Vec<Vec<JsonValue>>) -> ConnectorResult<()> {
        if self.rows.len() + rows.len() > self.limit {
            return Err(ConnectorError::Query(format!(
                "Expected at most {} row(s).",
                self.limit
            )));
        }
        self.rows.extend(rows);
        Ok(())
    }
}

/// Batches rows for a sink: `push` one row, `flush` at the end.
pub struct RowBatcher {
    batch: Vec<Vec<JsonValue>>,
}

impl Default for RowBatcher {
    fn default() -> Self {
        Self {
            batch: Vec::with_capacity(STREAM_BATCH_ROWS),
        }
    }
}

impl RowBatcher {
    pub async fn push(
        &mut self,
        sink: &mut dyn RowSink,
        control: &Arc<QueryControl>,
        row: Vec<JsonValue>,
    ) -> ConnectorResult<()> {
        self.batch.push(row);
        if self.batch.len() >= STREAM_BATCH_ROWS {
            stream_cancelled(control)?;
            sink.rows(std::mem::take(&mut self.batch)).await?;
            self.batch.reserve(STREAM_BATCH_ROWS);
        }
        Ok(())
    }

    pub async fn flush(&mut self, sink: &mut dyn RowSink) -> ConnectorResult<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        sink.rows(std::mem::take(&mut self.batch)).await
    }
}
