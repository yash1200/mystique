//! Collection structure sections (fields, indexes, options, stats).

use mongodb::bson::{doc, Document};
use mystique_connector_api::{
    cell_bool, ConnectorError, ConnectorResult, ObjectStructure, StructureSection,
};

use super::catalog;
use super::MongoSession;

pub(super) async fn describe_object_structure(
    session: &MongoSession,
    database: Option<&str>,
    _schema: Option<&str>,
    table: &str,
    kind: Option<&str>,
) -> ConnectorResult<ObjectStructure> {
    session.ensure_open()?;
    let db_name = session.resolve_database(database)?;
    let db = session.client.database(&db_name);
    let coll = db.collection::<Document>(table);

    let cols = catalog::describe_table(session, Some(&db_name), None, table).await?;
    let field_rows: Vec<Vec<String>> = cols
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.data_type.clone(),
                cell_bool(c.nullable),
                cell_bool(c.pk),
            ]
        })
        .collect();
    let fields_sec = StructureSection::table(
        "fields",
        "Fields",
        vec![
            "Name".into(),
            "Sample type".into(),
            "Nullable".into(),
            "Id".into(),
        ],
        field_rows,
    )
    .with_description("Inferred from up to 50 sample documents.")
    .with_empty_message("No fields sampled (empty collection).");

    let mut index_rows = Vec::new();
    let mut cursor = coll
        .list_indexes()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    while cursor
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let model = cursor
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        let opts = model.options.as_ref();
        let name = opts
            .and_then(|o| o.name.clone())
            .unwrap_or_else(|| "—".into());
        let key = model
            .keys
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let unique = opts.and_then(|o| o.unique).unwrap_or(false);
        let sparse = opts.and_then(|o| o.sparse).unwrap_or(false);
        index_rows.push(vec![
            name,
            if key.is_empty() { "—".into() } else { key },
            cell_bool(unique),
            cell_bool(sparse),
        ]);
    }
    let indexes_sec = StructureSection::table(
        "indexes",
        "Indexes",
        vec![
            "Name".into(),
            "Key".into(),
            "Unique".into(),
            "Sparse".into(),
        ],
        index_rows,
    )
    .with_empty_message("No indexes.");

    // Collection options / validator from listCollections.
    let mut validation_rows = Vec::new();
    let mut listed = db
        .list_collections()
        .filter(doc! { "name": table })
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    while listed
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let spec = listed
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        validation_rows.push(vec!["type".into(), format!("{:?}", spec.collection_type)]);
        let options = &spec.options;
        if let Some(validator) = &options.validator {
            validation_rows.push(vec![
                "validator".into(),
                serde_json::to_string_pretty(validator)
                    .unwrap_or_else(|_| format!("{validator:?}")),
            ]);
        }
        if let Some(level) = &options.validation_level {
            validation_rows.push(vec!["validationLevel".into(), format!("{level:?}")]);
        }
        if let Some(action) = &options.validation_action {
            validation_rows.push(vec!["validationAction".into(), format!("{action:?}")]);
        }
        if let Some(capped) = options.capped {
            validation_rows.push(vec!["capped".into(), cell_bool(capped)]);
        }
    }
    let options_sec = StructureSection::table(
        "options",
        "Collection options",
        vec!["Property".into(), "Value".into()],
        validation_rows,
    )
    .with_empty_message("No special options / validator.");

    let mut stats_rows = Vec::new();
    let count = coll
        .estimated_document_count()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    stats_rows.push(vec!["estimatedDocumentCount".into(), count.to_string()]);
    let stats_sec = StructureSection::table(
        "stats",
        "Stats",
        vec!["Metric".into(), "Value".into()],
        stats_rows,
    );

    let object_kind = kind.unwrap_or("collection").to_string();
    Ok(ObjectStructure::new(
        format!("{db_name}.{table}"),
        vec![fields_sec, indexes_sec, options_sec, stats_sec],
    )
    .with_subtitle(format!("MongoDB · {object_kind}"))
    .with_object_kind(object_kind))
}
