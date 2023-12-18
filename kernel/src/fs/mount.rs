use alloc::{boxed::Box, sync::Arc};

use crate::fs::{
    dentry::DEntry,
    path::PathBuf,
    vfs::{FSResult, FileSystem},
};

pub struct MountCtx {
    pub fs: Box<dyn FileSystem + Send + Sync>,
    pub dest: Option<DEntry>,
    pub source: Option<PathBuf>,
}

pub enum MountType {
    // BlockDevice,
    NoDevice,
}

// pub fn mount_bdev(fs: Box<dyn FileSystem>, )

pub fn mount_nodev(
    mut fs: Box<dyn FileSystem + Send + Sync>,
) -> FSResult<Arc<dyn FileSystem + Send + Sync>> {
    fs.init_super()?;
    Ok(Arc::from(fs))
}
