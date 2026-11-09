//! Mongo shell text: one parser shared by the executor, the write policy and
//! the guard preview.
//!
//! - [`lex`] turns text into tokens. Strings and comments end here.
//! - [`value`] reads literals (`{ … }`, `[ … ]`, `ObjectId(…)`) and makes BSON.
//! - [`parse`] reads a statement and lowers it to the call the executor runs.
//! - [`policy`] classifies the parsed statement for the write guard.
//! - [`preview`] names the statement for the guard prompt.
//! - [`split`] cuts a buffer into statements for Run all.

mod lex;
pub(crate) mod parse;
pub(crate) mod policy;
mod preview;
pub(crate) mod split;
pub(crate) mod value;

pub use policy::MongoQueryPolicy;

pub(crate) use parse::{
    parse, parse_executable, BulkModel, Executable, FindSpec, Listing, MongoCall, MongoOp,
    ShellWrite, Target,
};
pub(crate) use value::{json_to_document, parse_document};

#[cfg(test)]
mod tests;
