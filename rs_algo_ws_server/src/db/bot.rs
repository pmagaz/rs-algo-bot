use mongodb::error::Error;
use mongodb::options::{FindOneAndReplaceOptions, FindOneOptions};
use mongodb::results::InsertOneResult;
pub use mongodb::Client;
use rs_algo_shared::helpers::uuid::Uuid;
use rs_algo_shared::models::bot::BotData;
use std::env;

use bson::doc;

pub struct Db {
    pub client: Client,
    pub name: String,
}

// pub fn get_collection_name(collection: &str, time_frame: &str) -> String {
//     let arr_str = collection.split('_').collect::<Vec<_>>();
//     let time_frame_code = arr_str.last().unwrap();

//     collection.replace(time_frame_code, time_frame)
// }
// pub async fn get_collection<T>(db: &str, collection: &str) -> Collection<T> {
//     client.database(db).collection::<T>(collection)
// }

pub async fn find_by_uuid(client: &Client, uuid: &Uuid) -> Option<BotData> {
    let db_name = &env::var("MONGO_BOT_DB_NAME").unwrap();
    let collection_name = &env::var("DB_BOT_COLLECTION").unwrap();
    let collection = client
        .database(db_name)
        .collection::<BotData>(collection_name);

    

    collection
        .find_one(doc! { "_id": uuid}, FindOneOptions::builder().build())
        .await
        .unwrap()
}

pub async fn insert(client: &Client, bot_data: &BotData) -> Result<InsertOneResult, Error> {
    let db_name = &env::var("MONGO_BOT_DB_NAME").unwrap();
    let collection_name = &env::var("DB_BOT_COLLECTION").unwrap();
    let collection = client
        .database(db_name)
        .collection::<BotData>(collection_name);

    collection.insert_one(bot_data, None).await
}

pub async fn upsert(client: &Client, doc: &BotData) -> Result<Option<BotData>, Error> {
    let db_name = &env::var("MONGO_BOT_DB_NAME").unwrap();
    let collection_name = &env::var("DB_BOT_COLLECTION").unwrap();
    let collection = client
        .database(db_name)
        .collection::<BotData>(collection_name);

    collection
        .find_one_and_replace(
            doc! {"_id": *doc.uuid()},
            doc,
            FindOneAndReplaceOptions::builder()
                .upsert(Some(true))
                .build(),
        )
        .await
}
