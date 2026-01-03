use crate::filestore::FileStore;
use crate::merkle_tree::manifest::ManifestFile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
// NEW: Match server's FileInfo response

#[derive(Debug, Deserialize, Serialize)]
struct FileInfoResponse {
    name: String,
    size: i64,
    tier: u8,
}

// NEW: Match server's manifest response wrapper
#[derive(Debug, Deserialize)]
struct ManifestResponse {
    manifest: ManifestFile,
}

pub trait SegmentSource: Send + Sync {
    fn list_files(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_manifest(&self, filename: &str) -> Result<ManifestFile, Box<dyn std::error::Error>>;
    fn read_segment(
        &self,
        filename: &str,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn read_block_segment(
        &self,
        filename: &str,
        block_id: usize,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn read_parity(
        &self,
        filename: &str,
        segment_id: usize,
        parity_id: usize,
        block_id: Option<usize>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn write_parity(
        &self,
        filename: &str,
        segment_id: usize,
        block_id: Option<usize>,
        recovered_bytes: &Vec<u8>,
    ) -> Result<bool, Box<dyn std::error::Error>>;
    fn read_data(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}

pub struct LocalSource {
    store: FileStore,
}

impl LocalSource {
    pub fn new(archive_path: PathBuf) -> Result<Self, std::io::Error> {
        let store = FileStore::new(&archive_path)?;
        Ok(Self { store })
    }
}

impl SegmentSource for LocalSource {
    fn list_files(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let files = self.store.get_all()?;
        Ok(files.iter().map(|f| f.file_name.clone()).collect())
    }

    fn get_manifest(&self, filename: &str) -> Result<ManifestFile, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;
        Ok(file.manifest)
    }

    fn read_segment(
        &self,
        filename: &str,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;
        let path = self.store.get_segment_path(&file, segment_id)?;
        Ok(std::fs::read(path)?)
    }

    fn read_block_segment(
        &self,
        filename: &str,
        block_id: usize,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;
        let path = self
            .store
            .get_block_segment_path(&file, block_id, segment_id)?;
        Ok(std::fs::read(path)?)
    }

    fn read_parity(
        &self,
        filename: &str,
        segment_id: usize,
        parity_id: usize,
        block_id: Option<usize>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;

        match &file.manifest.tier {
            1 => {
                let parity_bytes = fs::read(self.store.get_parity_path_t1(&file, parity_id)?)?;
                Ok(parity_bytes)
            }
            2 => {
                let parity_bytes = fs::read(
                    self.store
                        .get_parity_path_t2(&file, segment_id, parity_id)?,
                )?;
                Ok(parity_bytes)
            }
            3 => {
                let block_id =
                    block_id.ok_or_else(|| "block_id is required for tier 3 parity reads")?;

                let parity_bytes =
                    fs::read(self.store.get_parity_path_t3(&file, block_id, parity_id)?)?;
                Ok(parity_bytes)
            }

            _ => Err("unknown tier".into()),
        }
    }

    fn write_parity(
        &self,
        filename: &str,
        segment_id: usize,
        block_id: Option<usize>,
        recovered_bytes: &Vec<u8>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;

        match &file.manifest.tier {
            1 => {
                let file_path = Path::new(&file.file_data.path)
                    .parent()
                    .ok_or("No parent directory")?;
                let data_path = file_path.join("data.dat");
                fs::write(&data_path, &recovered_bytes)?;
                Ok(true)
            }
            2 => {
                let file_path = Path::new(&file.file_data.path)
                    .parent()
                    .ok_or("No parent directory")?
                    .join("segments")
                    .join(format!("segment_{}.dat", segment_id));
                fs::write(file_path, recovered_bytes)?;

                Ok(true)
            }
            3 => {
                let block_id =
                    block_id.ok_or_else(|| "block_id is required for tier 3 parity reads")?;

                let file_path = Path::new(&file.file_data.path)
                    .parent()
                    .ok_or("No parent directory")?
                    .join("blocks")
                    .join(format!("block_{}", block_id))
                    .join("segments")
                    .join(format!("segment_{}.dat", segment_id));
                fs::write(file_path, recovered_bytes)?;

                Ok(true)
            }

            _ => Err("unknown tier".into()),
        }
    }

    fn read_data(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let file = self.store.find(&filename.to_string())?;
        let file_bytes = fs::read(self.store.get_data_path(&file)?)?;
        Ok(file_bytes)
    }
}

pub struct RemoteSource {
    base_url: String,
    agent: ureq::Agent,
}

impl RemoteSource {
    pub fn new(base_url: String) -> Self {
        let agent = ureq::Agent::new_with_defaults();

        Self { base_url, agent }
    }
}

impl SegmentSource for RemoteSource {
    fn list_files(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/files", self.base_url);
        let response: Vec<FileInfoResponse> = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_json()?;
        Ok(response.into_iter().map(|f| f.name).collect())
    }

    fn get_manifest(&self, filename: &str) -> Result<ManifestFile, Box<dyn std::error::Error>> {
        let url = format!("{}/api/files/{}/manifest", self.base_url, filename);
        let response: ManifestResponse = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_json()?;
        Ok(response.manifest)
    }

    fn read_segment(
        &self,
        filename: &str,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/files/{}/segment/{}",
            self.base_url, filename, segment_id
        );

        let response: Vec<u8> = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_to_vec()?;

        Ok(response)
    }

    fn read_block_segment(
        &self,
        filename: &str,
        block_id: usize,
        segment_id: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/files/{}/block/{}/segment/{}",
            self.base_url, filename, block_id, segment_id
        );
        let response: Vec<u8> = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_to_vec()?;

        Ok(response)
    }

    fn read_parity(
        &self,
        filename: &str,
        segment_id: usize,
        parity_id: usize,
        block_id: Option<usize>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = if let Some(bid) = block_id {
            format!(
                "{}/api/files/{}/parity/?block_id={}&segment_id={}&parity_id={}",
                self.base_url, filename, bid, segment_id, parity_id
            )
        } else {
            format!(
                "{}/api/files/{}/parity/?segment_id={}&parity_id={}",
                self.base_url, filename, segment_id, parity_id
            )
        };
        let response: Vec<u8> = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_to_vec()?;

        Ok(response)
    }

    fn write_parity(
        &self,
        filename: &str,
        segment_id: usize,
        block_id: Option<usize>,
        _recovered_bytes: &Vec<u8>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/files/{}/parity/?block_id={}&segment_id={}",
            self.base_url,
            filename,
            block_id.unwrap_or(0),
            segment_id,
        );
        self.agent.get(&url).call()?;
        Ok(true)
    }

    fn read_data(&self, filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/files/{}", self.base_url, filename);
        let response: Vec<u8> = self
            .agent
            .get(&url)
            .call()?
            .body_mut()
            .with_config()
            .read_to_vec()?;
        Ok(response)
    }
}
