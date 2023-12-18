use alloc::{sync::Arc, vec::Vec};

use spin::lock_api::RwLock;

use crate::fs::{mount::MountType, path::PathBuf, vfs::FSResult};

pub mod dentry;
// pub mod ext2;
pub mod mount;
pub mod path;
pub mod ramfs;
pub mod vfs;

pub static MOUNTS: Mounts = Mounts::new();

pub struct Mounts {
    mounts: RwLock<Vec<Mount>>,
}

struct Mount {
    fs: Arc<dyn vfs::FileSystem + Send + Sync>,
    dentry: dentry::DEntry,
    tp: MountType,
}

impl Mounts {
    pub const fn new() -> Self {
        Self {
            mounts: RwLock::new(Vec::new()),
        }
    }

    pub fn mount_fs(&self, mut ctx: mount::MountCtx) -> FSResult<()> {
        let fs = match ctx.fs.mount_type() {
            MountType::NoDevice => mount::mount_nodev(ctx.fs)?,
        };

        let dentry = match ctx.dest.take() {
            Some(dentry) => dentry,
            None => dentry::DEntry::new(
                PathBuf::from("/"),
                fs.superblock().read().root()?,
                Arc::clone(&fs),
            ),
        };

        // Add the mount to the mount table
        self.mounts.write().push(Mount {
            tp: fs.mount_type(),
            fs: Arc::clone(&fs),
            dentry: dentry.clone(),
        });

        // Cache the root inode
        dentry::DIR_CACHE.mount(dentry);

        Ok(())
    }

    pub fn is_mount_path(&self, path: &path::Path) -> bool {
        self.mounts
            .read()
            .iter()
            .any(|mount| &*mount.dentry.name() == path)
    }
}
