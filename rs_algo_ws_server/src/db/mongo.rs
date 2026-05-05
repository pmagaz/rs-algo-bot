use mongodb::{bson::doc, options::ClientOptions, Client};

use crate::error::RsAlgoError;

pub async fn connect(
    username: &str,
    password: &str,
    db_name: &str,
    uri: &str,
) -> Result<Client, RsAlgoError> {
    let db_uri = if username.is_empty() {
        uri.to_string()
    } else {
        format!("mongodb://{}:{}@{}", username, password, uri)
    };

    tracing::info!("MongoDB: connecting to '{}'", uri);

    let client_options = ClientOptions::parse(&db_uri).await.unwrap();
    let client = Client::with_options(client_options).unwrap();

    client
        .database("admin")
        .run_command(doc! {"ping": 1}, None)
        .await
        .unwrap();

    tracing::info!("MongoDB: server ready, using database '{}'", db_name);

    Ok(client)
}
