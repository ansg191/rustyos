mod error;
pub mod file_iter;

use alloc::{boxed::Box, sync::Arc};
use core::{
    any::Any,
    fmt::{Debug, Formatter},
};

use bitflags::bitflags;
use spin::lock_api::RwLock;

pub use self::error::*;
use crate::fs::{
    dentry::DEntry,
    mount::MountType,
    path::{Component, Path},
    vfs::file_iter::FileIter,
};

pub trait FileSystem {
    fn name(&self) -> &str;

    fn mount_type(&self) -> MountType;

    fn init_super(&mut self) -> FSResult<()>;

    /// Gets the superblock of the file system
    fn superblock(&self) -> Arc<RwLock<dyn SuperBlock + Send + Sync>>;
}

pub trait SuperBlock: Any {
    fn root(&self) -> FSResult<Inode>;

    /// Creates a new inode on the file system
    fn create_inode(&mut self) -> FSResult<Inode>;

    /// Gets an inode from the file system
    fn get_inode(&self, inode_n: u64) -> FSResult<Option<Inode>>;

    /// Destroys an inode on the file system
    fn destroy_inode(&mut self, inode_n: u64) -> FSResult<()>;

    /// Writes an inode to the file system
    /// Make sure to reload the dentry after writing the inode
    fn write_inode(&mut self, inode: &Inode) -> FSResult<()>;
}

/// Operations that can be performed on an inode
///
/// Operations are not allowed to modify and commit changes to parent inodes, and
/// they are not allowed to commit changes to the inode itself.
///
/// Users must manually commit changes to the inode and its parent inodes.
pub trait InodeOps {
    /// Creates a regular file in `dst` with `parent` and `path`
    fn create(&self, dst: &mut Inode, parent: &DEntry, path: Component) -> FSResult<()>;
    /// Creates a hard link to `src` in `parent` + `path`
    fn link(&self, src: &mut Inode, parent: &DEntry, path: Component) -> FSResult<()>;
    /// Creates a symbolic link in `dst` to `src` with `parent` & `path`
    fn symlink(
        &self,
        dst: &mut Inode,
        src: &Path,
        parent: &DEntry,
        path: Component,
    ) -> FSResult<()>;
    /// Unlinks `dst` from `parent`
    fn unlink(&self, dst: &mut Inode, parent: &DEntry) -> FSResult<()>;
    /// Renames `src` to `dst` with `src_p` & `dst_p`
    fn rename(
        &self,
        src: &mut Inode,
        src_p: &DEntry,
        dst_p: &DEntry,
        path: Component,
    ) -> FSResult<()>;

    fn mkdir(&self, dst: &mut Inode, parent: &DEntry, path: Component) -> FSResult<()>;
    fn list<'b>(&self, inode: &'b Inode) -> FSResult<FileIter<'b>>;
}

pub struct Inode {
    /// The type of the inode
    pub(super) mode: Mode,
    /// The permissions of the inode
    pub(super) permission: Permission,
    /// The user id of the inode
    pub(super) user_id: u16,
    /// The group id of the inode
    pub(super) group_id: u16,

    /// The number of the inode
    pub(super) num: u64,

    /// The size of the inode in bytes
    pub(super) size: u64,
    /// The number of hard links to the inode
    pub(super) nlink: u16,

    /// The number of blocks used by the inode
    pub(super) blocks: u64,

    /// The time the inode was last accessed
    pub(super) last_access_time: u64,
    /// The time the inode was created
    pub(super) creation_time: u64,
    /// The time the inode was last modified
    pub(super) last_modification_time: u64,

    /// Inode operations
    pub(super) ops: &'static (dyn InodeOps + Send + Sync),

    /// Private data for the file system
    pub(super) private: Box<dyn Any + Send + Sync>,
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
    pub struct Mode: u8 {
        const FIFO = 1 << 0;
        const CHARACTER_DEVICE = 1 << 1;
        const DIRECTORY = 1 << 2;
        const BLOCK_DEVICE = 1 << 3;
        const REGULAR_FILE = 1 << 4;
        const SYMBOLIC_LINK = 1 << 5;
        const SOCKET = 1 << 6;
    }
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
    pub struct Permission: u16 {
        const OTHER_EXECUTE = 1 << 0;
        const OTHER_WRITE = 1 << 1;
        const OTHER_READ = 1 << 2;
        const GROUP_EXECUTE = 1 << 3;
        const GROUP_WRITE = 1 << 4;
        const GROUP_READ = 1 << 5;
        const USER_EXECUTE = 1 << 6;
        const USER_WRITE = 1 << 7;
        const USER_READ = 1 << 8;
        const STICKY = 1 << 9;
    }
}

impl Inode {
    #[inline]
    pub fn ops(&self) -> &'static (dyn InodeOps + Send + Sync) {
        self.ops
    }

    #[inline]
    pub const fn is_dir(&self) -> bool {
        self.mode.contains(Mode::DIRECTORY)
    }

    #[inline]
    pub fn create(&mut self, parent: &DEntry, path: Component) -> FSResult<()> {
        self.ops.create(self, parent, path)
    }

    #[inline]
    pub fn link(&mut self, parent: &DEntry, path: Component) -> FSResult<()> {
        self.ops.link(self, parent, path)
    }

    #[inline]
    pub fn symlink(&mut self, src: &Path, parent: &DEntry, path: Component) -> FSResult<()> {
        self.ops.symlink(self, src, parent, path)
    }

    #[inline]
    pub fn unlink(&mut self, parent: &DEntry) -> FSResult<()> {
        self.ops.unlink(self, parent)
    }

    #[inline]
    pub fn rename(&mut self, src_p: &DEntry, dst_p: &DEntry, path: Component) -> FSResult<()> {
        self.ops.rename(self, src_p, dst_p, path)
    }

    #[inline]
    pub fn mkdir(&mut self, parent: &DEntry, path: Component) -> FSResult<()> {
        self.ops.mkdir(self, parent, path)
    }

    #[inline]
    pub fn list(&self) -> FSResult<FileIter> {
        self.ops.list(self)
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl Debug for Inode {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Inode")
            .field("mode", &self.mode)
            .field("permission", &self.permission)
            .field("user_id", &self.user_id)
            .field("group_id", &self.group_id)
            .field("inode_n", &self.num)
            .field("size", &self.size)
            .field("nlink", &self.nlink)
            .field("blocks", &self.blocks)
            .field("last_access_time", &self.last_access_time)
            .field("creation_time", &self.creation_time)
            .field("last_modification_time", &self.last_modification_time)
            .finish()
    }
}
