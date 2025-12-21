use tracing::error;
// Windows WinFSP implementation for BlockframeFS
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY};
use winfsp::filesystem::{
    DirBuffer, DirInfo, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo, VolumeInfo,
    WideNameInfo,
};
use winfsp::{FspError, Result, U16CStr, U16CString};

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use super::cache::SegmentCache;
use super::source::SegmentSource;
use crate::merkle_tree::manifest::ManifestFile;

// File context for open files
pub struct BlockframeFileContext {
    filename: String,
    _cursor: u64,
    dir_buffer: Option<DirBuffer>,
}

// Main filesystem structure
pub struct BlockframeFS {
    inner: Arc<Mutex<BlockframeFSInner>>,
}

// Inner filesystem state
struct BlockframeFSInner {
    source: Box<dyn SegmentSource>,
    cache: SegmentCache,
    inode_to_filename: HashMap<u64, String>,
    filename_to_inode: HashMap<String, u64>,
    next_inode: u64,
    manifests: HashMap<String, ManifestFile>,
}

impl BlockframeFS {
    pub fn new(source: Box<dyn SegmentSource>) -> Result<Self> {
        let cache_capacity = 1_000_000_000;
        let mut inner = BlockframeFSInner {
            source,
            cache: SegmentCache::new_with_byte_limit(cache_capacity),
            inode_to_filename: HashMap::new(),
            filename_to_inode: HashMap::new(),
            next_inode: 2, // 1 is root
            manifests: HashMap::new(),
        };

        // Initialize file list
        let _ = inner.refresh_files();

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }
}

impl BlockframeFSInner {
    fn refresh_files(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let files = self.source.list_files()?;
        for filename in files {
            if !self.filename_to_inode.contains_key(&filename) {
                let inode = self.next_inode;
                self.next_inode += 1;
                self.inode_to_filename.insert(inode, filename.clone());
                self.filename_to_inode.insert(filename.clone(), inode);

                if let Ok(manifest) = self.source.get_manifest(&filename) {
                    self.manifests.insert(filename, manifest);
                }
            }
        }
        Ok(())
    }

    fn get_file_info(&self, filename: &str) -> Option<FileInfo> {
        let manifest = self.manifests.get(filename)?;

        Some(FileInfo {
            file_attributes: FILE_ATTRIBUTE_READONLY.0,
            reparse_tag: 0,
            allocation_size: ((manifest.size as u64 + 511) / 512) * 512,
            file_size: manifest.size as u64,
            creation_time: 0,
            last_access_time: 0,
            last_write_time: 0,
            change_time: 0, // TODO: Get from manifest
            index_number: *self.filename_to_inode.get(filename).unwrap_or(&0),
            hard_links: 1,
            ea_size: 0,
        })
    }

    fn read_from_source(
        &mut self,
        filename: &str,
        segment_index: usize,
        tier: u8,
    ) -> std::result::Result<Arc<Vec<u8>>, Box<dyn std::error::Error>> {
        let cache_key = format!("{}:{}", filename, segment_index);
        if let Some(segment) = self.cache.get(&cache_key) {
            return Ok(segment);
        }

        let segment_data = match tier {
            1 => self.source.read_data(filename),
            2 => self.source.read_segment(filename, segment_index),
            3 => {
                let block_size = 30;
                let block_index = segment_index / block_size;
                let segment_in_block = segment_index % block_size;
                self.source
                    .read_block_segment(filename, block_index, segment_in_block)
            }
            _ => Err("Unsupported tier".into()),
        }?;

        let segment = Arc::new(segment_data);
        self.cache.put(cache_key, segment.clone());
        Ok(segment)
    }
}

impl FileSystemContext for BlockframeFS {
    type FileContext = BlockframeFileContext;

    fn get_volume_info(&self, volume_info: &mut VolumeInfo) -> Result<()> {
        volume_info.total_size = 1024 * 1024 * 1024 * 1024;
        volume_info.free_size = 1024 * 1024 * 1024 * 1024;
        volume_info.set_volume_label("BlockframeFS");
        Ok(())
    }

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity> {
        let filename = file_name.to_string_lossy();

        if filename == "\\" {
            return Ok(FileSecurity {
                attributes: FILE_ATTRIBUTE_DIRECTORY.0,
                reparse: false,
                sz_security_descriptor: 0,
            });
        }

        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let clean_name = filename.trim_start_matches('\\');

        if inner.manifests.contains_key(clean_name) {
            Ok(FileSecurity {
                attributes: FILE_ATTRIBUTE_READONLY.0,
                reparse: false,
                sz_security_descriptor: 0,
            })
        } else {
            Err(FspError::NTSTATUS(-1073741772)) // STATUS_OBJECT_NAME_NOT_FOUND
        }
    }

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: u32,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext> {
        let filename = file_name.to_string_lossy();

        // Handle root directory
        if filename == "\\" {
            let info: &mut FileInfo = file_info.as_mut();
            info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            info.allocation_size = 0;
            info.file_size = 0;
            info.creation_time = 0;
            info.last_access_time = 0;
            info.last_write_time = 0;
            info.change_time = 0;
            info.index_number = 1;
            info.reparse_tag = 0;
            info.hard_links = 1;
            info.ea_size = 0;

            return Ok(Self::FileContext {
                filename: "\\".to_string(),
                _cursor: 0,
                dir_buffer: Some(DirBuffer::new()),
            });
        }

        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let clean_name = filename.trim_start_matches('\\');

        if let Some(info) = inner.get_file_info(clean_name) {
            *file_info.as_mut() = info;
            Ok(BlockframeFileContext {
                filename: clean_name.to_string(),
                _cursor: 0,
                dir_buffer: None,
            })
        } else {
            Err(FspError::NTSTATUS(-1073741772))
        }
    }

    fn close(&self, _context: Self::FileContext) {
        // Nothing to clean up for read-only filesystem
    }

    fn read(
        &self,
        file_context: &Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<u32> {
        let manifest = {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match inner.manifests.get(&file_context.filename) {
                Some(manifest) => manifest.clone(),
                None => return Err(FspError::NTSTATUS(-1073741772)),
            }
        };

        if offset >= manifest.size as u64 {
            return Ok(0);
        }

        let bytes_to_read = (manifest.size as u64 - offset).min(buffer.len() as u64) as usize;
        let mut bytes_read = 0;
        let mut current_offset = offset;

        while bytes_read < bytes_to_read {
            let segment_size = manifest.segment_size;
            let segment_index = (current_offset / segment_size) as usize;
            let segment_offset = (current_offset % segment_size) as usize;

            // FIXED: Arc is scoped and drops immediately after copy
            let copy_len = {
                let mut inner = self
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let segment_data = inner
                    .read_from_source(&file_context.filename, segment_index, manifest.tier)
                    .map_err(|_| FspError::NTSTATUS(-1073741772))?;

                let len = (segment_data.len() - segment_offset).min(bytes_to_read - bytes_read);

                // Copy data while holding Arc
                buffer[bytes_read..bytes_read + len]
                    .copy_from_slice(&segment_data[segment_offset..segment_offset + len]);

                len
            }; // Arc drops HERE - refcount back to 1, LRU can evict

            bytes_read += copy_len;
            current_offset += copy_len as u64;
        }

        Ok(bytes_read as u32)
    }

    fn read_directory(
        &self,
        file_context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: winfsp::filesystem::DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        if file_context.filename != "\\" {
            return Err(FspError::NTSTATUS(-1073741808));
        }

        let dir_buffer = file_context
            .dir_buffer
            .as_ref()
            .expect("dir_buffer must be initialized before use");

        if let Ok(dir_buffer_lock) = dir_buffer.acquire(marker.is_none(), None) {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut dir_info: DirInfo = DirInfo::new();
            for filename in inner.manifests.keys() {
                if let Some(mut file_info) = inner.get_file_info(filename) {
                    dir_info.reset();
                    let file_name_u16 = match U16CString::from_str(filename) {
                        Ok(name) => name,
                        Err(_) => continue,
                    };
                    if dir_info.set_name_cstr(&file_name_u16).is_err() {
                        // filename too long, just skip it
                        continue;
                    }
                    *dir_info.file_info_mut() = file_info.clone();

                    if dir_buffer_lock.write(&mut dir_info).is_err() {
                        // buffer is full
                        break;
                    }
                }
            }
        }

        Ok(dir_buffer.read(marker, buffer))
    }

    fn get_file_info(
        &self,
        file_context: &Self::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<()> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if file_context.filename == "\\" {
            *file_info = FileInfo {
                file_attributes: FILE_ATTRIBUTE_DIRECTORY.0,
                allocation_size: 0,
                file_size: 0,
                creation_time: 0,
                last_access_time: 0,
                last_write_time: 0,
                change_time: 0,
                index_number: 1,
                reparse_tag: 0,
                hard_links: 1,
                ea_size: 0,
            };
        } else if let Some(info) = inner.get_file_info(&file_context.filename) {
            *file_info = info;
        } else {
            return Err(FspError::NTSTATUS(-1073741772));
        }

        Ok(())
    }
}
