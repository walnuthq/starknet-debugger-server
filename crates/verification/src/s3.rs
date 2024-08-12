use anyhow::Result;
use sqlx::{Pool, Postgres};

use crate::db::fetch_verified_class;
use crate::{VerifiedClassData, VerifiedClassRow};

pub fn key_for_class_hash(class_hash: String) -> String {
    format!("class-{}.json", class_hash)
}

pub async fn fetch_verified_class_with_data(
    db_pool: &Pool<Postgres>,
    s3_client: &aws_sdk_s3::Client,
    class_hash: String,
) -> Result<(VerifiedClassRow, VerifiedClassData)> {
    let verified_class = fetch_verified_class(db_pool, class_hash.clone()).await?;

    let resp = s3_client
        .get_object()
        .bucket("walnutserver-east-1-classes-verification")
        .key(key_for_class_hash(class_hash))
        .send()
        .await?;

    let body = resp.body.collect().await?;
    let parsed: VerifiedClassData = serde_json::from_slice(&body.into_bytes())?;

    Ok((verified_class, parsed))
}
