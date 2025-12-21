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
    fn io_to_poem(
        &self,
        err: Box<dyn std::error::Error>,
        msg: &str,
        status: StatusCode,
    ) -> poem::Error {
        tracing::error!("{}: {}", msg, err);
        poem::Error::from_string(err.to_string(), StatusCode::BAD_REQUEST)
    }
    // list all files in archive
    #[oai(path = "/files", method = "get")]
    async fn list_files(&self) -> Result<Json<Vec<FileInfo>>, poem::Error> {
        let store = self.store.read();
        let files = store.get_all().map_err(|err: Box<dyn std::error::Error>| {
            self.io_to_poem(
                err,
                "Failed to fetch files",
                StatusCode::INTERNAL_SERVER_ERROR,
            )
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
                self.io_to_poem(
                    err,
                    &format!("Failed to find file {}", filename.0),
                    StatusCode::NOT_FOUND,
                )
            })?
            .manifest;

        Ok(Json(
            json!({
                "manifest": manifest
            })
            .to_json()
            .ok_or_else(|| {
                let err =
                    std::io::Error::new(std::io::ErrorKind::Other, "JSON serialization failed");
                self.io_to_poem(
                    Box::new(err),
                    &format!("Failed to serialize manifest for file {}", filename.0),
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
                self.io_to_poem(
                    err,
                    &format!("Failed to find file {}", filename.0),
                    StatusCode::NOT_FOUND,
                )
            })?;
        let data_path = store.get_data_path(&file_obj).map_err(|err| {
            self.io_to_poem(
                Box::new(err),
                &format!("Invalid data path for file {}", filename.0),
                StatusCode::BAD_REQUEST,
            )
        })?;

        let file_bytes = fs::read(data_path).map_err(|err| {
            self.io_to_poem(
                Box::new(err),
                &format!("Failed to find file {}", filename.0),
                StatusCode::NOT_FOUND,
            )
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
                self.io_to_poem(
                    err,
                    &format!("Failed to find file {}", filename.0),
                    StatusCode::NOT_FOUND,
                )
            })?;

        let segment_path = store
            .get_segment_path(&file_obj, segment_id.0)
            .map_err(|err| {
                self.io_to_poem(
                    Box::new(err),
                    &format!(
                        "Failed to get segment path for file {} segment {}",
                        filename.0, segment_id.0
                    ),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?;

        let file_bytes = fs::read(&segment_path).map_err(|err| {
            self.io_to_poem(
                Box::new(err),
                &format!(
                    "Failed to read segment {:?} for file {}",
                    segment_id.0, filename.0
                ),
                StatusCode::NOT_FOUND,
            )
        })?;
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

        let block_segment_path = store
            .get_block_segment_path(&file_obj, block_id.0, segment_id.0)
            .map_err(|err| {
                self.io_to_poem(
                    Box::new(err),
                    &format!("Failed to get block segment path for file {}", filename.0),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?;

        let file_bytes = fs::read(&block_segment_path).map_err(|err| {
            self.io_to_poem(
                Box::new(err),
                &format!("Failed to find block segment {}", filename.0),
                StatusCode::NOT_FOUND,
            )
        })?;

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
                let parity_id = parity_id.0.ok_or_else(|| {
                    poem::Error::from_string("Missing parity_id", StatusCode::BAD_REQUEST)
                })?;
                let parity_path = store
                    .get_parity_path_t1(&file_obj, parity_id)
                    .map_err(|e| {
                        poem::Error::from_string(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
                    })?;
                let parity_bytes = fs::read(parity_path).map_err(|err: std::io::Error| {
                    tracing::error!("Failed to find file {}: {}", filename.0, err);
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;
                return Ok(Binary(parity_bytes));
            }
            2 => {
                let segment_id = segment_id.0.ok_or_else(|| {
                    poem::Error::from_string("Missing segment_id", StatusCode::BAD_REQUEST)
                })?;
                let parity_id = parity_id.0.ok_or_else(|| {
                    poem::Error::from_string("Missing parity_id", StatusCode::BAD_REQUEST)
                })?;
                let parity_path = store
                    .get_parity_path_t2(&file_obj, segment_id, parity_id)
                    .map_err(|e| {
                        poem::Error::from_string(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
                    })?;
                let parity_bytes = fs::read(parity_path).map_err(|err: std::io::Error| {
                    tracing::error!(
                        "Failed to find segment {:?} for file {}: {}",
                        segment_id,
                        filename.0,
                        err
                    );
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;
                return Ok(Binary(parity_bytes));
            }
            3 => {
                let block_id = block_id.0.ok_or_else(|| {
                    poem::Error::from_string("block_id is required", StatusCode::BAD_REQUEST)
                })?;
                let parity_id = parity_id.0.ok_or_else(|| {
                    poem::Error::from_string("Missing parity_id", StatusCode::BAD_REQUEST)
                })?;

                if (&file_obj.manifest.merkle_tree.leaves.len() as &usize) - 1 < block_id {
                    return Err(poem::Error::from_string(
                        "block_id is out of range",
                        StatusCode::BAD_REQUEST,
                    ));
                }

                if parity_id > 2 {
                    return Err(poem::Error::from_string(
                        "tier 3 files have only 3 parity shards. Parity index out of range",
                        StatusCode::BAD_REQUEST,
                    ));
                }
                if let Some(sid) = segment_id.0 {
                    if sid > 30 {
                        return Err(poem::Error::from_string(
                            "tier 3 files have only 30 segments. Segment index out of range",
                            StatusCode::BAD_REQUEST,
                        ));
                    }
                }
                let parity_path = store
                    .get_parity_path_t3(&file_obj, block_id, parity_id)
                    .map_err(|e| {
                        poem::Error::from_string(e.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
                    })?;
                let parity_bytes = fs::read(parity_path).map_err(|err: std::io::Error| {
                    tracing::error!("Failed to find block segment {}: {}", filename.0, err);
                    poem::Error::from_string(err.to_string(), StatusCode::NOT_FOUND)
                })?;

                return Ok(Binary(parity_bytes));
            }
            _ => Ok(Binary(vec![0])),
        }
    }
}
