//! Collection create / rename / drop / truncate / duplicate.

use mongodb::bson::{doc, Document};
use mongodb::Client;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, TableAdminInput, TableAdminOp, TableAdminResult,
};
use serde_json::json;

use super::MongoSession;

pub(super) async fn apply_table_admin(
    session: &MongoSession,
    input: &TableAdminInput,
) -> ConnectorResult<TableAdminResult> {
    session.ensure_open()?;
    let db_name = session.resolve_database(input.database.as_deref())?;
    let db = session.client.database(&db_name);
    let name = input.name.trim();
    let coll = db.collection::<Document>(name);

    match input.op {
        TableAdminOp::CreateStatement => {
            let statement = format!(
                "// Collection {name} in database {db_name}\ndb.createCollection({});\n",
                json!(name)
            );
            Ok(TableAdminResult::statement(input.op, statement))
        }
        TableAdminOp::CreateGroup => Err(ConnectorError::NotSupported(
            "MongoDB does not use schemas; create a collection instead.".into(),
        )),
        TableAdminOp::CreateObject => {
            db.create_collection(name)
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Created collection {name} in {db_name}."),
            ))
        }
        TableAdminOp::Rename => {
            let new_name = input.new_name.as_deref().unwrap_or("").trim();
            mongo_rename_collection(&session.client, &db_name, name, new_name).await?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Renamed {name} to {new_name}."),
            ))
        }
        TableAdminOp::Drop => {
            coll.drop()
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Dropped collection {name}."),
            ))
        }
        TableAdminOp::Truncate => {
            // Native clear: delete every document, keep the collection + indexes.
            coll.delete_many(doc! {})
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Truncated collection {name}."),
            ))
        }
        TableAdminOp::Duplicate => {
            let new_name = input.new_name.as_deref().unwrap_or("").trim();
            mongo_copy_collection(&db, name, new_name).await?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Duplicated {name} to {new_name}."),
            ))
        }
    }
}

async fn mongo_rename_collection(
    client: &Client,
    db_name: &str,
    from: &str,
    to: &str,
) -> ConnectorResult<()> {
    let renamed = client
        .database("admin")
        .run_command(doc! {
            "renameCollection": format!("{db_name}.{from}"),
            "to": format!("{db_name}.{to}"),
            "dropTarget": false,
        })
        .await;
    if renamed.is_ok() {
        return Ok(());
    }
    // Atlas / restricted users often cannot run renameCollection on admin.
    // Fall back to copy + drop so the SPI still works.
    let db = client.database(db_name);
    mongo_copy_collection(&db, from, to).await?;
    db.collection::<Document>(from)
        .drop()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    Ok(())
}

async fn mongo_copy_collection(
    db: &mongodb::Database,
    from: &str,
    to: &str,
) -> ConnectorResult<()> {
    if to.is_empty() || to == from {
        return Err(ConnectorError::InvalidConfig(
            "Duplicate/rename target name is invalid.".into(),
        ));
    }
    let src = db.collection::<Document>(from);
    let dest = db.collection::<Document>(to);
    let _ = db.create_collection(to).await;
    let mut cursor = src
        .find(doc! {})
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    let mut batch: Vec<Document> = Vec::new();
    while cursor
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let doc = cursor
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        batch.push(doc);
        if batch.len() >= 500 {
            dest.insert_many(std::mem::take(&mut batch))
                .await
                .map_err(|e| ConnectorError::Query(e.to_string()))?;
        }
    }
    if !batch.is_empty() {
        dest.insert_many(batch)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
    }
    Ok(())
}
