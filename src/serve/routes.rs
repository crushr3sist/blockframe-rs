use parking_lot::RwLock;
use poem::http::StatusCode;
use poem_openapi::{
    Object, OpenApi,
    param::Path,
    param::Query,
    payload::{Binary, Json},
    types::ToJSON,
};
use serde_json::json;
use std::path::Path as Path_Native;
use std::{fs, sync::Arc};

use crate::filestore::FileStore;

#[derive(Object)]
pub struct FileInfo {
    name: String,
    size: i64,
    tier: u8,
}

pub struct BlockframeApi {
    store: Arc<RwLock<FileStore>>,
}
impl BlockframeApi {
    pub fn new(store: FileStore) -> Self {
        Self {
            store: Arc::new(RwLock::new(store)),
        }
    }
}

#[OpenApi]
impl BlockframeApi {
    // list all files in archive
    #[oai(path = "/files", method = "get")]
    async fn list_files(&self) -> poem::Result<Json<Vec<FileInfo>>> {
        let store = self.store.read();
        let files = store.get_all().map_err(|err: Box<dyn std::error::Error>| {
            // 1. "Check" / Inspect the error here
            println!("Database exploded: {:?}", err);
            tracing::error!("Failed to fetch files: {}", err);

            // 2. Return the Poem error to the client
            poem::Error::from_string(err.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
        })?; // <--- The '?' operator propagates the error to the endpoint return type
        Ok(Json(
            files
                .iter()
                .map(|f| FileInfo {
                    name: f.file_name.clone(),
                    size: f.manifest.size,
                    tier: f.manifest.tier,
                })
                .collect(),
        ))
    }

    // get file manifest
    #[oai(path = "/files/:filename/manifest", method = "get")]
    async fn get_manifest(
        &self,
        filename: Path<String>,
    ) -> Result<Json<serde_json::Value>, poem::Error> {
        // return manifest.json content
        let store = self.store.read();
        let manifest = store
            .find(&filename)
            .map_err(|err: Box<dyn std::error::Error>| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?
            .manifest;

        Ok(Json(
            json!({
                "manifest": manifest
            })
            .to_json()
            .ok_or_else(|| {
                tracing::error!("Failed to serialize manifest for file {}", filename.0);
                poem::Error::from_string(
                    "Failed to serialize manifest",
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?,
        ))
    }
    // get segment data
    #[oai(path = "/files/:filename", method = "get")]
    async fn get_data(&self, filename: Path<String>) -> Result<Binary<Vec<u8>>, poem::Error> {
        let store = self.store.read();

        let file_obj = store
            .find(&filename)
            .map_err(|err: Box<dyn std::error::Error>| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?;

        let file_bytes =
            fs::read(store.get_data_path(&file_obj)).map_err(|err: std::io::Error| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?;

        return Ok(Binary(file_bytes));
    }

    // get segment data
    #[oai(path = "/files/:filename/segment/:segment_id", method = "get")]
    async fn get_segment(
        &self,
        filename: Path<String>,
        segment_id: Path<usize>,
    ) -> Result<Binary<Vec<u8>>, poem::Error> {
        let store = self.store.read();

        let file_obj = store
            .find(&filename)
            .map_err(|err: Box<dyn std::error::Error>| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?;

        let file_bytes = fs::read(store.get_segment_path(&file_obj, segment_id.0)).map_err(
            |err: std::io::Error| {
                tracing::error!(
                    "Failed to find segment {:?} for file {}: {}",
                    segment_id.0,
                    filename.0,
                    err
                );
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            },
        )?;
        return Ok(Binary(file_bytes));
    }

    // get block segment (Tier 3)
    #[oai(
        path = "/files/:filename/block/:block_id/segment/:segment_id",
        method = "get"
    )]
    async fn get_block_segment(
        &self,
        filename: Path<String>,
        block_id: Path<usize>,
        segment_id: Path<usize>,
    ) -> Result<Binary<Vec<u8>>, poem::Error> {
        // read and return segment bytes
        let store = self.store.read();

        let file_obj = store
            .find(&filename)
            .map_err(|err: Box<dyn std::error::Error>| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?;

        let file_bytes =
            fs::read(store.get_block_segment_path(&file_obj, block_id.0, segment_id.0)).map_err(
                |err: std::io::Error| {
                    tracing::error!("Failed to find block segment {}: {}", filename.0, err);
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                },
            )?;

        return Ok(Binary(file_bytes));
    }

    // get parity shard
    #[oai(path = "/files/:filename/parity/", method = "get")]
    async fn get_parity(
        &self,
        filename: Path<String>,
        block_id: Query<Option<usize>>,
        segment_id: Query<Option<usize>>,
        parity_id: Query<Option<usize>>,
    ) -> Result<Binary<Vec<u8>>, poem::Error> {
        let store = self.store.read();

        let file_obj = store
            .find(&filename)
            .map_err(|err: Box<dyn std::error::Error>| {
                tracing::error!("Failed to find file {}: {}", filename.0, err);
                poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
            })?;

        match file_obj.manifest.tier {
            1 => {
                let parity_bytes = fs::read(
                    store.get_parity_path_t1(&file_obj, parity_id.0.unwrap()),
                )
                .map_err(|err: std::io::Error| {
                    tracing::error!("Failed to find file {}: {}", filename.0, err);
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;
                return Ok(Binary(parity_bytes));
            }
            2 => {
                let parity_bytes = fs::read(store.get_parity_path_t2(
                    &file_obj,
                    segment_id.0.unwrap(),
                    parity_id.0.unwrap(),
                ))
                .map_err(|err: std::io::Error| {
                    tracing::error!(
                        "Failed to find segment {:?} for file {}: {}",
                        segment_id.0,
                        filename.0,
                        err
                    );
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;
                return Ok(Binary(parity_bytes));
            }
            3 => {
                if block_id.is_none() {
                    return Err(poem::Error::from_string(
                        "block_id is required for this file",
                        StatusCode::BAD_REQUEST,
                    ));
                }
                if (&file_obj.manifest.merkle_tree.leaves.len() as &usize) - 1 < block_id.0.unwrap()
                {
                    return Err(poem::Error::from_string(
                        "block_id is out of range",
                        StatusCode::BAD_REQUEST,
                    ));
                }

                if parity_id.0.unwrap() > 2 {
                    return Err(poem::Error::from_string(
                        "tier 3 files have only 3 parity shards. Parity index out of range",
                        StatusCode::BAD_REQUEST,
                    ));
                }
                if segment_id.0.unwrap() > 30 {
                    return Err(poem::Error::from_string(
                        "tier 3 files have only 30 segments. Segment index out of range",
                        StatusCode::BAD_REQUEST,
                    ));
                }
                let parity_bytes = fs::read(store.get_parity_path_t3(
                    &file_obj,
                    segment_id.0.unwrap(),
                    block_id.0.unwrap(),
                    parity_id.0.unwrap(),
                ))
                .map_err(|err: std::io::Error| {
                    tracing::error!("Failed to find block segment {}: {}", filename.0, err);
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;

                return Ok(Binary(parity_bytes));
            }
            _ => Ok(Binary(vec![0])),
        }
    }
}
