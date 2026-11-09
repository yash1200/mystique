//! Driver option structs from a parsed call, and the time limit every command
//! gets. Pure, so tests assert the exact structs the driver receives.
//!
//! Time limit: the profile query timeout (0 = none), the same value the SQL
//! engines send as `statement_timeout` / `max_execution_time`. A `maxTimeMS`
//! the user typed can lower it, never raise it.

use std::time::Duration;

use mongodb::bson::{self, Bson, Document};
use mongodb::options::{
    AggregateOptions, Collation, CountOptions, DistinctOptions, EstimatedDocumentCountOptions,
    FindOneAndDeleteOptions, FindOneAndReplaceOptions, FindOneAndUpdateOptions, FindOptions, Hint,
    ReturnDocument, WriteConcern,
};
use mystique_connector_api::{ConnectorError, ConnectorResult, NO_TIMEOUT};

use crate::shell::{AggregateSpec, CountSpec, FindAndModify, FindSpec, ReadOptions};

/// The server time limit for one command: the profile timeout, lowered by
/// the user's `maxTimeMS`. `None` sends no `maxTimeMS`.
pub(super) fn max_time(timeout_ms: u64, user_ms: Option<u64>) -> Option<Duration> {
    let profile = (timeout_ms != NO_TIMEOUT).then_some(timeout_ms);
    let ms = match (profile, user_ms.filter(|ms| *ms > 0)) {
        (Some(profile), Some(user)) => profile.min(user),
        (Some(ms), None) | (None, Some(ms)) => ms,
        (None, None) => return None,
    };
    Some(Duration::from_millis(ms))
}

/// [`max_time`] as the `maxTimeMS` field of a command document.
pub(super) fn max_time_field(timeout_ms: u64, user_ms: Option<u64>) -> Option<i64> {
    max_time(timeout_ms, user_ms).map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

pub(super) fn hint(value: &Bson) -> ConnectorResult<Hint> {
    match value {
        Bson::String(name) => Ok(Hint::Name(name.clone())),
        Bson::Document(keys) => Ok(Hint::Keys(keys.clone())),
        other => Err(ConnectorError::Query(format!(
            "hint must be an index name or a key pattern, got {other}"
        ))),
    }
}

fn hint_opt(value: Option<&Bson>) -> ConnectorResult<Option<Hint>> {
    value.map(hint).transpose()
}

fn collation(doc: Option<&Document>) -> ConnectorResult<Option<Collation>> {
    doc.map(|doc| {
        bson::from_document::<Collation>(doc.clone())
            .map_err(|err| ConnectorError::Query(format!("collation is not valid: {err}")))
    })
    .transpose()
}

fn write_concern(doc: Option<&Document>) -> ConnectorResult<Option<WriteConcern>> {
    doc.map(|doc| {
        bson::from_document::<WriteConcern>(doc.clone())
            .map_err(|err| ConnectorError::Query(format!("writeConcern is not valid: {err}")))
    })
    .transpose()
}

/// `limit` is the user's limit within the row cap; without one, the row cap.
pub(super) fn find_options(
    spec: &FindSpec,
    max_rows: u32,
    timeout_ms: u64,
) -> ConnectorResult<FindOptions> {
    let cap = i64::from(max_rows.max(1));
    let mut options = FindOptions::default();
    options.projection = spec.projection.clone();
    options.sort = spec.sort.clone();
    options.skip = spec.skip;
    // A negative limit is a single batch of that many, as in the shell.
    options.limit = Some(spec.limit.map(i64::abs).unwrap_or(cap).clamp(1, cap));
    options.batch_size = spec.batch_size;
    options.allow_disk_use = spec.allow_disk_use;
    options.hint = hint_opt(spec.read.hint.as_ref())?;
    options.collation = collation(spec.read.collation.as_ref())?;
    options.comment = spec.read.comment.clone();
    options.max_time = max_time(timeout_ms, spec.read.max_time_ms);
    Ok(options)
}

/// The total behind a find: the same filter, hint and collation.
pub(super) fn find_count_options(
    spec: &FindSpec,
    timeout_ms: u64,
) -> ConnectorResult<CountOptions> {
    let mut options = CountOptions::default();
    options.hint = hint_opt(spec.read.hint.as_ref())?;
    options.collation = collation(spec.read.collation.as_ref())?;
    options.max_time = max_time(timeout_ms, spec.read.max_time_ms);
    Ok(options)
}

/// Without a `batchSize`, one batch holds up to the row cap (at most 1000).
pub(super) fn aggregate_options(
    spec: &AggregateSpec,
    max_rows: u32,
    timeout_ms: u64,
) -> ConnectorResult<AggregateOptions> {
    let mut options = AggregateOptions::default();
    options.batch_size = Some(spec.batch_size.unwrap_or(max_rows.clamp(1, 1_000)));
    options.allow_disk_use = spec.allow_disk_use;
    options.bypass_document_validation = spec.bypass_document_validation;
    options.let_vars = spec.let_vars.clone();
    options.hint = hint_opt(spec.read.hint.as_ref())?;
    options.collation = collation(spec.read.collation.as_ref())?;
    options.comment = spec.read.comment.clone();
    options.max_time = max_time(timeout_ms, spec.read.max_time_ms);
    Ok(options)
}

pub(super) fn count_options(spec: &CountSpec, timeout_ms: u64) -> ConnectorResult<CountOptions> {
    let mut options = CountOptions::default();
    options.limit = spec.limit;
    options.skip = spec.skip;
    options.hint = hint_opt(spec.read.hint.as_ref())?;
    options.collation = collation(spec.read.collation.as_ref())?;
    options.comment = spec.read.comment.clone();
    options.max_time = max_time(timeout_ms, spec.read.max_time_ms);
    Ok(options)
}

pub(super) fn estimated_count_options(
    read: &ReadOptions,
    timeout_ms: u64,
) -> EstimatedDocumentCountOptions {
    let mut options = EstimatedDocumentCountOptions::default();
    options.comment = read.comment.clone();
    options.max_time = max_time(timeout_ms, read.max_time_ms);
    options
}

pub(super) fn distinct_options(
    read: &ReadOptions,
    timeout_ms: u64,
) -> ConnectorResult<DistinctOptions> {
    let mut options = DistinctOptions::default();
    options.collation = collation(read.collation.as_ref())?;
    options.comment = read.comment.clone();
    options.max_time = max_time(timeout_ms, read.max_time_ms);
    Ok(options)
}

fn return_document(after: Option<bool>) -> Option<ReturnDocument> {
    after.map(|after| {
        if after {
            ReturnDocument::After
        } else {
            ReturnDocument::Before
        }
    })
}

pub(super) fn find_one_and_update_options(
    spec: &FindAndModify,
    timeout_ms: u64,
) -> ConnectorResult<FindOneAndUpdateOptions> {
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = spec.upsert;
    options.return_document = return_document(spec.return_after);
    options.projection = spec.projection.clone();
    options.sort = spec.sort.clone();
    options.array_filters = spec.array_filters.clone();
    options.collation = collation(spec.collation.as_ref())?;
    options.hint = hint_opt(spec.hint.as_ref())?;
    options.bypass_document_validation = spec.options.bypass_document_validation;
    options.let_vars = spec.options.let_vars.clone();
    options.comment = spec.options.comment.clone();
    options.write_concern = write_concern(spec.options.write_concern.as_ref())?;
    options.max_time = max_time(timeout_ms, spec.options.max_time_ms);
    Ok(options)
}

pub(super) fn find_one_and_replace_options(
    spec: &FindAndModify,
    timeout_ms: u64,
) -> ConnectorResult<FindOneAndReplaceOptions> {
    let mut options = FindOneAndReplaceOptions::default();
    options.upsert = spec.upsert;
    options.return_document = return_document(spec.return_after);
    options.projection = spec.projection.clone();
    options.sort = spec.sort.clone();
    options.collation = collation(spec.collation.as_ref())?;
    options.hint = hint_opt(spec.hint.as_ref())?;
    options.bypass_document_validation = spec.options.bypass_document_validation;
    options.let_vars = spec.options.let_vars.clone();
    options.comment = spec.options.comment.clone();
    options.write_concern = write_concern(spec.options.write_concern.as_ref())?;
    options.max_time = max_time(timeout_ms, spec.options.max_time_ms);
    Ok(options)
}

pub(super) fn find_one_and_delete_options(
    spec: &FindAndModify,
    timeout_ms: u64,
) -> ConnectorResult<FindOneAndDeleteOptions> {
    let mut options = FindOneAndDeleteOptions::default();
    options.projection = spec.projection.clone();
    options.sort = spec.sort.clone();
    options.collation = collation(spec.collation.as_ref())?;
    options.hint = hint_opt(spec.hint.as_ref())?;
    options.let_vars = spec.options.let_vars.clone();
    options.comment = spec.options.comment.clone();
    options.write_concern = write_concern(spec.options.write_concern.as_ref())?;
    options.max_time = max_time(timeout_ms, spec.options.max_time_ms);
    Ok(options)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mongodb::bson::{doc, to_document, Bson};
    use mongodb::options::{Hint, ReturnDocument};

    use super::*;
    use crate::shell::{parse_executable, Executable, MongoOp, ShellWrite};

    fn op(text: &str) -> MongoOp {
        match parse_executable(text) {
            Ok(Executable::Call(call)) => call.op,
            other => panic!("{text}: {other:?}"),
        }
    }

    fn find(text: &str) -> FindSpec {
        match op(text) {
            MongoOp::Find(spec) => spec,
            other => panic!("{other:?}"),
        }
    }

    fn modify(text: &str) -> FindAndModify {
        match op(text) {
            MongoOp::Write(ShellWrite::FindAndModify(spec)) => spec,
            other => panic!("{other:?}"),
        }
    }

    fn ms(n: u64) -> Option<Duration> {
        Some(Duration::from_millis(n))
    }

    #[test]
    fn the_profile_timeout_caps_the_users_max_time() {
        assert_eq!(max_time(30_000, None), ms(30_000));
        assert_eq!(max_time(30_000, Some(500)), ms(500));
        assert_eq!(max_time(30_000, Some(90_000)), ms(30_000));
        // 0 = no profile timeout: only the user's limit, if any.
        assert_eq!(max_time(NO_TIMEOUT, None), None);
        assert_eq!(max_time(NO_TIMEOUT, Some(700)), ms(700));
        assert_eq!(max_time(NO_TIMEOUT, Some(0)), None);
        assert_eq!(max_time_field(1_500, None), Some(1_500));
        assert_eq!(max_time_field(NO_TIMEOUT, None), None);
    }

    #[test]
    fn find_options_carry_the_third_argument() {
        let spec = find(
            r#"db.t.find({a: 1}, {b: 1}, {sort: {b: -1}, skip: 4, limit: 20, hint: "a_1", collation: {locale: "fr", strength: 2}, batchSize: 7, allowDiskUse: true, comment: "why", maxTimeMS: 300})"#,
        );
        let o = match find_options(&spec, 1_000, 60_000) {
            Ok(o) => o,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(o.projection, Some(doc! { "b": 1 }));
        assert_eq!(o.sort, Some(doc! { "b": -1 }));
        assert_eq!(o.skip, Some(4));
        assert_eq!(o.limit, Some(20));
        assert_eq!(o.batch_size, Some(7));
        assert_eq!(o.allow_disk_use, Some(true));
        assert_eq!(o.hint, Some(Hint::Name("a_1".into())));
        let collation = o.collation.map(|c| to_document(&c).unwrap_or_default());
        assert_eq!(collation, Some(doc! { "locale": "fr", "strength": 2 }));
        assert_eq!(o.comment, Some(Bson::String("why".into())));
        assert_eq!(o.max_time, ms(300));
    }

    #[test]
    fn find_limit_stays_inside_the_row_cap() {
        let limit = |text: &str, cap: u32| {
            find_options(&find(text), cap, 1_000)
                .ok()
                .and_then(|o| o.limit)
        };
        assert_eq!(limit("db.t.find()", 500), Some(500));
        assert_eq!(limit("db.t.find().limit(5)", 500), Some(5));
        assert_eq!(limit("db.t.find().limit(-5)", 500), Some(5));
        assert_eq!(limit("db.t.find({}, {}, {limit: 9000})", 500), Some(500));
        assert_eq!(limit("db.t.findOne()", 500), Some(1));
        let o = find_options(&find("db.t.find()"), 500, NO_TIMEOUT).ok();
        assert_eq!(o.and_then(|o| o.max_time), None, "0 sends no maxTimeMS");
    }

    #[test]
    fn the_find_total_uses_the_finds_hint_and_collation() {
        let spec = find(r#"db.t.find({a: 1}).hint({a: 1}).collation({locale: "en"})"#);
        let o = match find_count_options(&spec, 2_000) {
            Ok(o) => o,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(o.hint, Some(Hint::Keys(doc! { "a": 1 })));
        assert!(o.collation.is_some());
        assert_eq!(o.max_time, ms(2_000));
        assert_eq!(o.limit, None);
        assert_eq!(o.skip, None);
    }

    #[test]
    fn aggregate_options_struct() {
        let MongoOp::Aggregate(spec) = op(
            r#"db.t.aggregate([{$match: {}}], {allowDiskUse: true, maxTimeMS: 90, collation: {locale: "en"}, hint: {a: 1}, batchSize: 50, let: {x: 1}, bypassDocumentValidation: true, comment: 3})"#,
        ) else {
            panic!("aggregate");
        };
        let o = match aggregate_options(&spec, 5_000, 10_000) {
            Ok(o) => o,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(o.allow_disk_use, Some(true));
        assert_eq!(o.max_time, ms(90));
        assert!(o.collation.is_some());
        assert_eq!(o.hint, Some(Hint::Keys(doc! { "a": 1 })));
        assert_eq!(o.batch_size, Some(50));
        assert_eq!(o.let_vars, Some(doc! { "x": 1 }));
        assert_eq!(o.bypass_document_validation, Some(true));
        assert_eq!(o.comment, Some(Bson::Int32(3)));

        let MongoOp::Aggregate(plain) = op("db.t.aggregate([])") else {
            panic!("aggregate");
        };
        let o = aggregate_options(&plain, 5_000, NO_TIMEOUT).unwrap_or_default();
        assert_eq!(o.batch_size, Some(1_000), "row cap, at most 1000");
        assert_eq!(o.max_time, None);
    }

    #[test]
    fn count_and_distinct_option_structs() {
        let MongoOp::Count(spec) =
            op(r#"db.t.countDocuments({a: 1}, {limit: 10, skip: 2, hint: "a_1", maxTimeMS: 40})"#)
        else {
            panic!("count");
        };
        let o = count_options(&spec, 1_000).unwrap_or_default();
        assert_eq!(o.limit, Some(10));
        assert_eq!(o.skip, Some(2));
        assert_eq!(o.hint, Some(Hint::Name("a_1".into())));
        assert_eq!(o.max_time, ms(40));

        let MongoOp::EstimatedCount(read) = op("db.t.estimatedDocumentCount({maxTimeMS: 5000})")
        else {
            panic!("estimated");
        };
        assert_eq!(estimated_count_options(&read, 1_000).max_time, ms(1_000));

        let MongoOp::Distinct { read, .. } =
            op(r#"db.t.distinct("a", {}, {collation: {locale: "de"}, comment: "d"})"#)
        else {
            panic!("distinct");
        };
        let o = distinct_options(&read, NO_TIMEOUT).unwrap_or_default();
        assert!(o.collation.is_some());
        assert_eq!(o.comment, Some(Bson::String("d".into())));
        assert_eq!(o.max_time, None);
    }

    #[test]
    fn find_one_and_update_options_struct() {
        let spec = modify(
            r#"db.t.findOneAndUpdate({a: 1}, {$set: {"x.$[e]": 1}}, {upsert: true, returnDocument: "after", projection: {x: 1}, sort: {a: -1}, arrayFilters: [{e: 1}], hint: "a_1", let: {v: 2}, bypassDocumentValidation: false, comment: "u", writeConcern: {w: "majority"}, maxTimeMS: 250})"#,
        );
        let o = match find_one_and_update_options(&spec, 1_000) {
            Ok(o) => o,
            Err(err) => panic!("{err}"),
        };
        assert_eq!(o.upsert, Some(true));
        assert!(matches!(o.return_document, Some(ReturnDocument::After)));
        assert_eq!(o.projection, Some(doc! { "x": 1 }));
        assert_eq!(o.sort, Some(doc! { "a": -1 }));
        assert_eq!(o.array_filters, Some(vec![doc! { "e": 1 }]));
        assert_eq!(o.hint, Some(Hint::Name("a_1".into())));
        assert_eq!(o.let_vars, Some(doc! { "v": 2 }));
        assert_eq!(o.bypass_document_validation, Some(false));
        assert_eq!(o.comment, Some(Bson::String("u".into())));
        let concern = o.write_concern.map(|w| to_document(&w).unwrap_or_default());
        assert_eq!(concern, Some(doc! { "w": "majority" }));
        assert_eq!(o.max_time, ms(250));
    }

    #[test]
    fn find_one_and_replace_and_delete_option_structs() {
        let replace = modify(
            "db.t.findOneAndReplace({a: 1}, {a: 2}, {returnNewDocument: false, upsert: true, sort: {a: 1}})",
        );
        let o = find_one_and_replace_options(&replace, 9_000).unwrap_or_default();
        assert!(matches!(o.return_document, Some(ReturnDocument::Before)));
        assert_eq!(o.upsert, Some(true));
        assert_eq!(o.sort, Some(doc! { "a": 1 }));
        assert_eq!(o.max_time, ms(9_000));

        let delete = modify(
            r#"db.t.findOneAndDelete({a: 1}, {projection: {_id: 1}, sort: {a: -1}, collation: {locale: "en"}})"#,
        );
        let o = find_one_and_delete_options(&delete, NO_TIMEOUT).unwrap_or_default();
        assert_eq!(o.projection, Some(doc! { "_id": 1 }));
        assert_eq!(o.sort, Some(doc! { "a": -1 }));
        assert!(o.collation.is_some());
        assert_eq!(o.max_time, None);

        let plain = modify("db.t.findOneAndUpdate({a: 1}, {$set: {b: 1}})");
        let o = find_one_and_update_options(&plain, 1_000).unwrap_or_default();
        assert!(o.return_document.is_none(), "the driver default (before)");
        assert_eq!(o.upsert, None);
    }
}
