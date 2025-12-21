use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use tracing::error;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};

use super::cache::SegmentCache;
use super::source::SegmentSource;

use crate::merkle_tree::manifest::{self, ManifestFile};

const TTL: Duration = Duration::from_secs(1);

pub struct BlockframeFS {
    source: Box<dyn SegmentSource>,
    cache: SegmentCache,

    // Inode mappings
    inode_to_filename: HashMap<u64, String>,
    filename_to_inode: HashMap<String, u64>,
    next_inode: u64,

    // Cached manifests
    manifests: HashMap<String, ManifestFile>,

    // open file handles (fh -> (filename, cursor position))
    open_files: HashMap<u64, (String, u64)>,
    next_fh: u64,

    uid: u32,
    gid: u32,
}

impl BlockframeFS {
    pub fn new(source: Box<dyn SegmentSource>) -> Result<Self, Box<dyn std::error::Error>> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let mut fs = Self {
            source,
            cache: SegmentCache::new(100),
            inode_to_filename: HashMap::new(),
            filename_to_inode: HashMap::new(),
            next_inode: 2, // 1 is root
            manifests: HashMap::new(),
            open_files: HashMap::new(),
            next_fh: 1,
            uid,
            gid,
        };

        // initialise file list
        fs.refresh_files()?;
        Ok(fs)
    }

    fn refresh_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let files = self.source.list_files()?;
        for filename in files {
            if !self.filename_to_inode.contains_key(&filename) {
                let inode = self.next_inode;
                self.next_inode += 1;
                self.inode_to_filename.insert(inode, filename.clone());
                self.filename_to_inode.insert(filename.clone(), inode);

                // cache manifest
                if let Ok(manifest) = self.source.get_manifest(&filename) {
                    self.manifests.insert(filename, manifest);
                }
            }
        }
        Ok(())
    }
    fn recover_segment(
        &self,
        filename: &str,
        manifest: &ManifestFile,
        segment_id: usize,
        block_id: Option<usize>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        println!("Recovering segment {} for {}", segment_id, filename);
        // fetch parity shards
        let parity_shards: Vec<Vec<u8>> = (0..3)
            .map(|i| self.source.read_parity(filename, segment_id, i, block_id))
            .collect::<Result<Vec<_>, _>>()?;
        // Reed-Solomon decode
        use reed_solomon_simd::ReedSolomonDecoder;
        let shard_size = parity_shards[0].len();
        let mut decoder = ReedSolomonDecoder::new(1, 3, shard_size)?;

        // add parity shards (data is missing/corrupt)
        decoder.add_recovery_shard(0, &parity_shards[0])?;

        let result = decoder.decode()?;
        let mut recovered = result
            .restored_original(0)
            .ok_or("Recovery failed")?
            .to_vec();

        // Tier 1 files are padded to 64 bytes for RS encoding, but the manifest size is the original size.
        if manifest.tier == 1 && recovered.len() > manifest.size as usize {
            recovered.truncate(manifest.size as usize);
        }

        // verify recovered data
        let expected_hash = if manifest.tier == 2 {
            &manifest
                .merkle_tree
                .segments
                .get(&segment_id)
                .ok_or("Missing segment info in manifest")?
                .data
        } else if manifest.tier == 3 {
            let block_id = block_id.ok_or("Block ID required for Tier 3 recovery")?;
            let seg_idx = segment_id % 30;
            manifest
                .merkle_tree
                .blocks
                .get(&block_id)
                .ok_or("Missing block info")?
                .segments
                .get(seg_idx)
                .ok_or("Missing segment hash")?
        } else {
            // Tier 1
            manifest
                .merkle_tree
                .leaves
                .get(&(segment_id as i32))
                .ok_or("Missing leaf hash")?
        };

        let actual_hash = crate::utils::sha256(&recovered)?;
        if actual_hash != *expected_hash {
            return Err("Recovery verification failed".into());
        }
        self.source
            .write_parity(filename, segment_id, block_id, &recovered)?;
        Ok(recovered)
    }
    fn get_file_attr(&self, filename: &str) -> Option<FileAttr> {
        let manifest = self.manifests.get(filename)?;
        let inode = *self.filename_to_inode.get(filename)?;

        Some(FileAttr {
            ino: inode,
            size: manifest.size as u64,
            blocks: (manifest.size as u64 + 511) / 512,
            atime: SystemTime::UNIX_EPOCH,
            mtime: SystemTime::UNIX_EPOCH,
            ctime: SystemTime::UNIX_EPOCH,
            crtime: SystemTime::UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o444, // READ ONLY
            nlink: 1,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 512,
            flags: 0,
        })
    }

    fn read_bytes(
        &mut self,
        filename: &str,
        segment_size: u64,
        tier: u8,
        offset: u64,
        size: usize,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // tier 1: whole file is one segment
        if tier == 1 {
            let mut data = self
                .cache.get_or_fetch(filename, 0, || self.source.read_data(filename))?.to_vec();

            // Verify integrity for Tier 1
            if let Some(manifest) = self.manifests.get(filename) {
                if let Some(expected_hash) = manifest.merkle_tree.leaves.get(&0) {
                    let actual_hash = crate::utils::sha256(&data)?;
                    if actual_hash != *expected_hash {
                        error!(
                            "Data corruption detected for {} (Tier 1). Attempting recovery...",
                            filename
                        );
                        data = self.recover_segment(filename, manifest, 0, None)?;
                    }
                }
            }

            let start = offset as usize;
            let end = std::cmp::min(start + size, data.len());
            return Ok(data[start..end].to_vec());
        }
        // tier 2 and 3: segmented
        let mut result = Vec::with_capacity(size);
        let mut remaining = size;
        let mut current_offset = offset;

        while remaining > 0 {
            let segment_id = (current_offset / segment_size) as usize;
            let offset_in_segment = (current_offset & segment_size) as usize;

            // fetch segment (from cache or source)
            let mut segment_data = if tier == 3 {
                let block_id = segment_id / 30;
                let segment_in_block = segment_id % 30;
                self.cache.get_or_fetch(
                    &format!("{}:block{}:seg{}", filename, block_id, segment_in_block), // The key for caching
                    segment_id,
                    || {
                        self.source
                            .read_block_segment(filename, block_id, segment_in_block)
                    },
                )?
            } else {
                self.cache.get_or_fetch(filename, segment_id, || { // The key for caching
                    self.source.read_segment(filename, segment_id)
                })?
            };

            let manifest = self
                .manifests
                .get(&filename.to_string())
                .ok_or("file not found in manifests hashtable line: 184 read_bytes")?;

            let expected_hash_opt = if tier == 2 {
                manifest
                    .merkle_tree
                    .segments
                    .get(&segment_id)
                    .map(|s| &s.data)
            } else if tier == 3 {
                let block_id = segment_id / 30;
                let seg_idx = segment_id % 30;
                manifest
                    .merkle_tree
                    .blocks
                    .get(&block_id)
                    .and_then(|b| b.segments.get(seg_idx))
            } else {
                manifest.merkle_tree.leaves.get(&(segment_id as i32))
            };

            let expected_hash = match expected_hash_opt {
                Some(h) => h,
                None => return Err(format!("Hash not found for segment {}", segment_id).into()),
            };

            let actual_hash = crate::utils::sha256(&segment_data)?;
            if tier == 3 {
                let block_id = segment_id / 30;
                if actual_hash != *expected_hash {
                    segment_data =
                        self.recover_segment(filename, manifest, segment_id, Some(block_id))?.into();
                }
            } else {
                if actual_hash != *expected_hash {
                    segment_data = self.recover_segment(filename, manifest, segment_id, None)?.into();
                }
            }

            // calculate how much we can read from this segment
            let available = segment_data.len() - offset_in_segment;
            let to_read = std::cmp::min(remaining, available);

            // append to result
            result.extend_from_slice(&segment_data[offset_in_segment..offset_in_segment + to_read]);
            remaining -= to_read;
            current_offset += to_read as u64;
        }
        Ok(result)
    }
}

impl Filesystem for BlockframeFS {
    /// Called when filesystem is mounted
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        println!("Blockframe filesystem mounted");

        Ok(())
    }

    /// get attributes of an inode
    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        if ino == 1 {
            // root directory
            let attr = FileAttr {
                ino: 1,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: self.uid,
                gid: self.gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.attr(&TTL, &attr);
        } else if let Some(filename) = self.inode_to_filename.get(&ino) {
            if let Some(attr) = self.get_file_attr(filename) {
                reply.attr(&TTL, &attr);
            } else {
                reply.error(libc::ENOENT);
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    /// Look up a directory entry by name
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if parent != 1 {
            reply.error(libc::ENOENT);
            return;
        }
        let filename = name.to_string_lossy().to_string();
        if let Some(attr) = self.get_file_attr(&filename) {
            reply.entry(&TTL, &attr, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    /// Read directory entries
    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino != 1 {
            reply.error(libc::ENOENT);
            return;
        }
        let entries: Vec<_> = vec![
            (1, FileType::Directory, "."),
            (1, FileType::Directory, ".."),
        ];

        let mut full_entries = entries;
        for (filename, inode) in &self.filename_to_inode {
            full_entries.push((*inode, FileType::RegularFile, filename.as_str()));
        }

        for (i, (ion, kind, name)) in full_entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*ion, (i + 1) as i64, *kind, name) {
                break;
            }
        }
        reply.ok();
    }

    /// Open a file
    fn open(&mut self, _req: &Request<'_>, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        if let Some(filename) = self.inode_to_filename.get(&ino).cloned() {
            let fh = self.next_fh;
            self.next_fh += 1;
            self.open_files.insert(fh, (filename, 0));
            reply.opened(fh, 0);
        }
    }

    // READ data from file - most important method
    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let (filename, _) = match self.open_files.get(&fh) {
            Some(f) => f.clone(),
            None => {
                reply.error(libc::EBADF);
                return;
            }
        };

        let (file_size, segment_size, tier) = match self.manifests.get(&filename) {
            Some(m) => (m.size as u64, m.segment_size as u64, m.tier as u8),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as u64;
        let size = size as u64;

        // Handle EOF
        if offset >= file_size {
            reply.data(&[]);
            return;
        }

        // calculate actual read size
        let actual_size = std::cmp::min(size, file_size - offset);

        // read segment(s) and slice
        match self.read_bytes(&filename, segment_size, tier, offset, actual_size as usize) {
            Ok(data) => reply.data(&data),
            Err(e) => {
                error!("Read error: {}", e);
                reply.error(libc::EIO);
            }
        }
    }

    /// Release (close) a file
    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        self.open_files.remove(&fh);
        reply.ok();
    }
}
