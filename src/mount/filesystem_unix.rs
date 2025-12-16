use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::time::{Duration, SystemTime};

use super::cache::SegmentCache;
use super::source::SegmentSource;

use crate::merkle_tree::manifest::ManifestFile;

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
}

impl BlockframeFS {
    pub fn new(source: Box<dyn SegmentSource>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut fs = Self {
            source,
            cache: SegmentCache::new(100),
            inode_to_filename: HashMap::new(),
            filename_to_inode: HashMap::new(),
            next_inode: 2, // 1 is root
            manifests: HashMap::new(),
            open_files: HashMap::new(),
            next_fh: 1,
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
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 512,
            flags: 0,
        })
    }
}
