//! Streaming storage provider that writes downloads to a hidden `.part` file
//! and the atomic rename that promotes it into the cache.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use stream_download::storage::StorageProvider;

pub(super) fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)
}

pub struct CacheFileProvider {
    /// Hidden temp file the download actually writes to.
    pub(super) part_path: PathBuf,
}

impl StorageProvider for CacheFileProvider {
    type Reader = File;
    type Writer = File;

    fn into_reader_writer(
        self,
        _content_length: Option<u64>,
    ) -> io::Result<(Self::Reader, Self::Writer)> {
        let writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&self.part_path)?;
        let reader = File::open(&self.part_path)?;
        Ok((reader, writer))
    }
}
