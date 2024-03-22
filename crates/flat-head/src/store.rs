use bytes::{Buf, Bytes, BytesMut};
use decoder::{decode_flat_files, handle_file};
use futures::stream::BoxStream;
use object_store::{aws::AmazonS3Builder, gcp::GoogleCloudStorageBuilder, path::Path, ObjectStore};
use std::{
    io::{self, Read},
    sync::Arc,
};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use url::Url;

use sf_protos::ethereum::r#type::v2::Block;

pub fn new<S: AsRef<str>>(store_url: S) -> Result<Store, anyhow::Error> {
    let url = Url::parse(store_url.as_ref())?;

    match url.scheme() {
        "gs" => {
            let bucket = url.host_str().ok_or_else(|| anyhow::anyhow!("No bucket"))?;
            let path = url.path();
            if path.starts_with("/") {}

            let store = GoogleCloudStorageBuilder::new()
                .with_bucket_name(bucket.to_string())
                .build()?;

            Ok(Store {
                store: Arc::new(store),
                base: match path.starts_with("/") {
                    false => path.to_string(),
                    true => path[1..].to_string(),
                },
            })
        }
        _ => return Err(anyhow::anyhow!("Unsupported scheme: {}", url.scheme())),
    }
}

pub struct Store {
    store: Arc<dyn ObjectStore>,
    base: String,
}

impl Store {
    pub async fn read_blocks(
        &self,
        path: &String,
        options: ReadOptions,
    ) -> Result<Vec<Block>, ReadError> {
        let content = self.store.get(&self.join_path(path)).await?;

        // FIXME: Use a version appropriate, we could use `content.into_store` and support async reader API.
        fake_handle_from_stream(content.into_stream(), options.decompress()).await
    }

    fn join_path(&self, path: &String) -> Path {
        Path::from(format!("{}/{}", self.base, path.trim_start_matches('/')))
    }
}

#[derive(Error, Debug)]
pub enum ReadError {
    #[error("Path '{0}' not found")]
    NotFound(String),
    #[error("Storage error: {0}")]
    Storage(#[from] object_store::Error),
}

pub struct ReadOptions {
    pub decompress: Option<bool>,
}

impl ReadOptions {
    pub fn decompress(&self) -> bool {
        self.decompress.unwrap_or(true)
    }
}

async fn fake_handle_from_stream(
    mut stream: BoxStream<'static, Result<Bytes, object_store::Error>>,
    decompress: bool,
) -> Result<Vec<Block>, ReadError> {
    use futures::stream::TryStreamExt; // for `try_next`

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open("/tmp/temp_block.dbin.zst")
        .await
        .expect("demo code, no file would be use when flat_file_decoders will be updated");

    while let Some(item) = stream.try_next().await? {
        file.write_all(&item)
            .await
            .expect("demo code, unable to write to temp file");
    }

    Ok(decode_flat_files(
        "/tmp/temp_block.dbin.zst".to_string(),
        None,
        None,
        Some(decompress),
    )
    .expect("demo code, deal with error nicely"))
}
