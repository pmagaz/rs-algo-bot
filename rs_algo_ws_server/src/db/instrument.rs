use bson::doc;
use futures::stream::StreamExt;
use mongodb::error::Error;
use mongodb::options::{FindOneAndReplaceOptions, FindOneOptions, FindOptions};
pub use mongodb::Client;
use mongodb::Collection;
use rs_algo_shared::models::instrument::*;

use std::env;

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

pub async fn find_by_symbol(client: &Client, symbol: &str) -> Result<Option<Instrument>, Error> {
    let db_name = &env::var("MONGO_BOT_DB_NAME").unwrap();
    let collection_name = &env::var("DB_BOT_COLLECTION").unwrap();
    let collection = client
        .database(db_name)
        .collection::<Instrument>(collection_name);

    let instrument = collection
        .find_one(doc! { "symbol": symbol}, FindOneOptions::builder().build())
        .await
        .unwrap();

    Ok(instrument)
}
