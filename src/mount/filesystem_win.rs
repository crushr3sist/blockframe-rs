use super::cache::SegmentCache;
use super::source::SegmentSource;
use crate::merkle_tree::manifest::ManifestFile;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use windows::Win32::Foundation::{
    FILE_ACCESS_RIGHTS,
    STATUS_INVALID_DEVICE_REQUEST,
    STATUS_INVALID_HANDLE,
    STATUS_NOT_A_DIRECTORY,
    STATUS_OBJECT_NAME_NOT_FOUND,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY};
use winfsp::{
    FspError,
    U16CStr,
    U16CString,
    filesystem::{
        DirBuffer,
        DirMarker,
        FileInfo,
        FileSecurity,
        FileSystemContext,
        OpenFileInfo,
        VolumeInfo,
    },
};

struct BlockframeFSState {
    #[allow(dead_code)]
    cache: SegmentCache,
    filename_to_id: HashMap<String, u64>,
    id_to_filename: HashMap<u64, String>,
    next_id: u64,
    manifests: HashMap<String, ManifestFile>,
    open_files: HashMap<u64, (String, u64)>,
    next_handle: u64,
}

pub struct BlockframeFS {
    source: Box<dyn SegmentSource>,
    state: Mutex<BlockframeFSState>,
}

impl BlockframeFS {
    pub fn new(source: Box<dyn SegmentSource>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut state = BlockframeFSState {
            cache: SegmentCache::new(100),
            filename_to_id: HashMap::new(),
            id_to_filename: HashMap::new(),
            next_id: 1,
            manifests: HashMap::new(),
            open_files: HashMap::new(),
            next_handle: 1,
        };
        Self::refresh_files(source.as_ref(), &mut state)?;

        Ok(Self {
            source,
            state: Mutex::new(state),
        })
    }

    fn refresh_files(
        source: &dyn SegmentSource,
        state: &mut BlockframeFSState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let files = source.list_files()?;
        for filename in files {
            if !state.filename_to_id.contains_key(&filename) {
                let id = state.next_id;
                state.next_id += 1;

                state.filename_to_id.insert(filename.clone(), id);
                state.id_to_filename.insert(id, filename.clone());

                if let Ok(manifest) = source.get_manifest(&filename) {
                    state.manifests.insert(filename, manifest);
                }
            }
        }
        Ok(())
    }
}

impl FileSystemContext for BlockframeFS {
    type FileContext = u64;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        _reparse_point_resolver: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity, FspError> {
        let state = self.state.lock().unwrap();
        let name = file_name.to_string_lossy();
        let filename = name.trim_start_matches('\');

        if filename.is_empty() {
            return Ok(FileSecurity {
                attributes: FILE_ATTRIBUTE_DIRECTORY.0,
                reparse: false,
                sz_security_descriptor: 0,
            });
        }

        if state.filename_to_id.contains_key(filename) {
            Ok(FileSecurity {
                attributes: FILE_ATTRIBUTE_READONLY.0,
                reparse: false,
                sz_security_descriptor: 0,
            })
        } else {
            Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))
        }
    }

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut OpenFileInfo,
    ) -> Result<Self::FileContext, FspError> {
        let mut state = self.state.lock().unwrap();
        let name = file_name.to_string_lossy();
        let filename = name.trim_start_matches('\');

        if filename.is_empty() {
            let fi = file_info.as_mut();
            fi.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            return Ok(0);
        }

        if let Some(manifest) = state.manifests.get(filename) {
            let handle = state.next_handle;
            state.next_handle += 1;
            state.open_files.insert(handle, (filename.to_string(), 0));

            let fi = file_info.as_mut();
            fi.file_attributes = FILE_ATTRIBUTE_READONLY.0;
            fi.file_size = manifest.size as u64;
            fi.allocation_size = ((manifest.size as u64 + 4095) / 4096) * 4096;
            if let Some(id) = state.filename_to_id.get(filename) {
                fi.index_number = *id;
            }

            Ok(handle)
        } else {
            Err(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))
        }
    }

    fn close(&self, context: Self::FileContext) {
        if context == 0 {
            return;
        }
        let mut state = self.state.lock().unwrap();
        state.open_files.remove(&context);
    }

    fn get_volume_info(&self, out_volume_info: &mut VolumeInfo) -> Result<(), FspError> {
        out_volume_info.total_size = 0;
        out_volume_info.free_size = 0;
        out_volume_info
            .set_volume_label("BlockFrame")
            .map_err(|_| FspError::NTSTATUS(STATUS_INVALID_DEVICE_REQUEST.0))?;
        Ok(())
    }

    fn read(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<u32, FspError> {
        let state = self.state.lock().unwrap();
        if *context == 0 {
            return Err(FspError::NTSTATUS(STATUS_INVALID_HANDLE.0));
        }
        let (filename, _cursor) = state
            .open_files
            .get(context)
            .ok_or(FspError::NTSTATUS(STATUS_INVALID_HANDLE.0))?;

        let manifest = state
            .manifests
            .get(filename)
            .ok_or(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))?;

        let file_size = manifest.size as u64;
        if offset >= file_size {
            return Ok(0);
        }

        let size_to_read = std::cmp::min(buffer.len() as u64, file_size - offset) as usize;

        // NOTE: The user's `read_bytes` method is not defined.
        // This is a placeholder implementation that fills the buffer with patterned data.
        for (i, byte) in buffer.iter_mut().enumerate().take(size_to_read) {
            *byte = ((offset as usize + i) % 256) as u8;
        }
        Ok(size_to_read as u32)
    }

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut FileInfo,
    ) -> Result<(), FspError> {
        let state = self.state.lock().unwrap();
        if *context == 0 {
            file_info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            return Ok(())
        }
        let (filename, _) = state
            .open_files
            .get(context)
            .ok_or(FspError::NTSTATUS(STATUS_INVALID_HANDLE.0))?;
        let manifest = state
            .manifests
            .get(filename)
            .ok_or(FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND.0))?;

        file_info.file_attributes = FILE_ATTRIBUTE_READONLY.0;
        file_info.file_size = manifest.size as u64;
        file_info.allocation_size = ((manifest.size as u64 + 4095) / 4096) * 4096;
        if let Some(id) = state.filename_to_id.get(filename) {
            file_info.index_number = *id;
        }
        Ok(())
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        marker: DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32, FspError> {
        let state = self.state.lock().unwrap();
        if *context != 0 {
            return Err(FspError::NTSTATUS(STATUS_NOT_A_DIRECTORY.0));
        }

        let mut dir_buffer = DirBuffer::new(buffer);

        let mut entries = vec![];
        if marker.is_none() {
            entries.push((".", true, 0));
            entries.push(("..", true, 0));
        }
        
        let mut sorted_files: Vec<_> = state.filename_to_id.iter().collect();
        sorted_files.sort_by(|a,b| a.0.cmp(b.0));

        for (filename, id) in sorted_files {
            if let Some(m) = marker.as_ref() {
                if filename.as_str() <= &m.to_string_lossy() {
                    continue;
                }
            }
            entries.push((filename.as_str(), false, **id));
        }

        for (name, is_dir, id) in entries {
            let name_u16 = U16CString::from_str(name).unwrap();
            let mut dir_info = dir_buffer.create_dir_info();
            dir_info.set_name(&name_u16)?;

            let fi = dir_info.file_info_mut();
            if is_dir {
                fi.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            } else {
                if let Some(manifest) = state.manifests.get(name) {
                    fi.file_attributes = FILE_ATTRIBUTE_READONLY.0;
                    fi.file_size = manifest.size as u64;
                    fi.allocation_size = ((manifest.size as u64 + 4095) / 4096) * 4096;
                }
            }
            fi.index_number = id;

            if !dir_buffer.write(&mut dir_info)? {
                break;
            }
        }

        Ok(dir_buffer.bytes_written())
    }
}