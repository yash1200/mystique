//! Mongo shell text: one parser shared by the executor, the write policy and
//! the guard preview.
//!
//! - [`lex`] turns text into tokens. Strings and comments end here.
//! - [`value`] reads literals (`{ … }`, `[ … ]`, `ObjectId(…)`) and makes BSON.
//! - [`parse`] reads a statement and lowers it to the call the executor runs.
//! - [`options`] reads a method's options argument against the keys it takes.
//! - [`policy`] classifies the parsed statement for the write guard.
//! - [`preview`] names the statement for the guard prompt.
//! - [`split`] cuts a buffer into statements for Run all.
//! - [`targets`] names the databases a write lands in (per-database production).

mod lex;
mod options;
pub(crate) mod parse;
pub(crate) mod policy;
mod preview;
pub(crate) mod split;
mod targets;
pub(crate) mod value;

pub use policy::MongoQueryPolicy;

pub(crate) use parse::{
    parse, parse_executable, AggregateSpec, CountSpec, DeleteModel, Executable, FindAndModify,
    FindSpec, Listing, Modify, MongoCall, MongoOp, ReadOptions, ShellWrite, Target, UpdateModel,
    WriteMethod, WriteModel, WriteOptions,
};
pub(crate) use value::{json_to_document, parse_document};

#[cfg(test)]
mod tests;
