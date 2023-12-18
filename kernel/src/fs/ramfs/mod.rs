use alloc::{boxed::Box, sync::Arc, vec::Vec};

use hashbrown::{hash_map::Entry, HashMap};
use spin::lock_api::{RwLock, RwLockReadGuard};
use static_assertions::assert_eq_size;

use crate::fs::{
    dentry::DEntry,
    mount::MountType,
    path::{Component, Path, PathBuf},
    vfs,
    vfs::{file_iter::FileIterator, FSResult},
};

const FS_NAME: &str = "ramfs";
const BLOCK_SIZE: usize = 0x1000;
const MAGIC: u64 = u64::from_be_bytes(*b"RAM_FS_M");

pub struct FileSystem {
    superblock: Arc<RwLock<SuperBlock>>,
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            superblock: Arc::new(RwLock::new(SuperBlock {
                root: 0,
                count: 0,
                inodes: HashMap::new(),
            })),
        }
    }
}

impl vfs::FileSystem for FileSystem {
    fn name(&self) -> &str {
        FS_NAME
    }

    fn mount_type(&self) -> MountType {
        MountType::NoDevice
    }

    fn init_super(&mut self) -> FSResult<()> {
        let mut superblock = self.superblock.write();

        let root = vfs::SuperBlock::create_inode(&mut *superblock)?;
        superblock.root = root.num;
        superblock.inodes.get_mut(&root.num).unwrap().mode = vfs::Mode::DIRECTORY;

        Ok(())
    }

    fn superblock(&self) -> Arc<RwLock<dyn vfs::SuperBlock + Send + Sync>> {
        Arc::clone(&self.superblock) as Arc<RwLock<dyn vfs::SuperBlock + Send + Sync>>
    }
}

struct SuperBlock {
    root: u64,
    count: u64,
    inodes: HashMap<u64, Inode>,
}

impl vfs::SuperBlock for SuperBlock {
    fn root(&self) -> FSResult<vfs::Inode> {
        Ok(vfs::Inode::from(self.inodes[&self.root].clone()))
    }

    fn create_inode(&mut self) -> FSResult<vfs::Inode> {
        let inode = Inode::default();

        let key = self.count;
        self.inodes.insert(key, inode);
        let inode = self.inodes.get_mut(&key).unwrap();

        inode.num = key;
        inode.creation_time = crate::time::TICKS.get();
        inode.last_access = inode.creation_time;
        inode.last_modification = inode.creation_time;

        self.count += 1;
        Ok(vfs::Inode::from(self.inodes[&key].clone()))
    }

    fn get_inode(&self, inode_n: u64) -> FSResult<Option<vfs::Inode>> {
        Ok(self
            .inodes
            .get(&inode_n)
            .map(|inode| vfs::Inode::from(inode.clone())))
    }

    fn destroy_inode(&mut self, _inode_n: u64) -> FSResult<()> {
        todo!()
    }

    fn write_inode(&mut self, inode: &vfs::Inode) -> FSResult<()> {
        let r_inode = inode
            .private
            .downcast_ref::<Inode>()
            .ok_or(vfs::FSError::WrongInode)?;

        match self.inodes.entry(r_inode.num) {
            Entry::Occupied(mut e) => {
                *e.get_mut() = r_inode.clone();
                Ok(())
            }
            Entry::Vacant(_) => Err(vfs::FSError::MissingInode),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Inode {
    mode: vfs::Mode,
    permission: vfs::Permission,
    user_id: u16,
    group_id: u16,

    num: u64,

    size: u64,
    nlink: u16,

    blocks: Arc<RwLock<Vec<Box<[u8; BLOCK_SIZE]>>>>,

    last_access: u64,
    creation_time: u64,
    last_modification: u64,
}

impl From<Inode> for vfs::Inode {
    fn from(value: Inode) -> Self {
        let blocks = value.blocks.read().len() as u64;
        Self {
            mode: value.mode,
            permission: value.permission,
            user_id: value.user_id,
            group_id: value.group_id,
            num: value.num,
            size: value.size,
            nlink: value.nlink,
            blocks,
            last_access_time: value.last_access,
            creation_time: value.creation_time,
            last_modification_time: value.last_modification,
            ops: &InodeOps,
            private: Box::new(value),
        }
    }
}

pub struct InodeOps;

impl InodeOps {
    fn append_dir_entry(inode: &Inode, entry: DirEntry) {
        let mut blocks = inode.blocks.write();
        let mut iter = blocks
            .iter_mut()
            .rev()
            .flat_map(|block| block.chunks_exact_mut(core::mem::size_of::<DirEntry>()))
            .map(|bytes| DirEntry::from_bytes_mut(bytes.try_into().unwrap()))
            .filter(|dir_entry| dir_entry.inode == 0);
        if let Some(e) = iter.next() {
            *e = entry;
        } else {
            let mut block = Box::new([0u8; BLOCK_SIZE]);
            block[..core::mem::size_of::<DirEntry>()]
                .copy_from_slice(&entry.to_bytes()[..core::mem::size_of::<DirEntry>()]);
            blocks.push(block);
        }
    }

    fn add_dir_entry<'a>(
        &'a self,
        dst: &'a mut vfs::Inode,
        i_vfs_parent: &'a mut vfs::Inode,
        path: &Component,
        inherit_permissions: bool,
    ) -> FSResult<(&'a mut Inode, &'a mut Inode)> {
        let Component::Normal(path) = path else {
            return Err(vfs::FSError::BadPath);
        };

        // Check if parent is a directory
        if i_vfs_parent.mode != vfs::Mode::DIRECTORY {
            return Err(vfs::FSError::NotDirectory);
        }

        // Check if the file already exists
        let iter = vfs::InodeOps::list(self, i_vfs_parent)?;
        for (p, _) in iter {
            if &*p == *path {
                return Err(vfs::FSError::Exists);
            }
        }

        let i_dst: &mut Inode = dst.private.downcast_mut().ok_or(vfs::FSError::WrongInode)?;
        let i_parent: &mut Inode = i_vfs_parent
            .private
            .downcast_mut()
            .ok_or(vfs::FSError::WrongInode)?;

        // Add file to parent directory
        let mut entry = DirEntry {
            inode: dst.num,
            length: path.len() as u8,
            name: [0; 247],
        };
        entry.name[..path.len()].copy_from_slice(path.as_bytes());

        Self::append_dir_entry(i_parent, entry);

        // Inherit permissions from parent
        if inherit_permissions {
            i_dst.permission = i_parent.permission;
        }

        // Update inode times
        let now = crate::time::TICKS.get();
        i_dst.last_modification = now;
        i_dst.last_access = now;
        i_parent.last_modification = now;
        i_parent.last_access = now;

        Ok((i_dst, i_parent))
    }

    fn create_impl(
        &self,
        dst: &mut vfs::Inode,
        i_vfs_parent: &mut vfs::Inode,
        path: &Component,
    ) -> FSResult<(vfs::Inode, vfs::Inode)> {
        let (i_dst, i_parent) = self.add_dir_entry(dst, i_vfs_parent, path, true)?;

        // Set to regular file
        i_dst.mode = vfs::Mode::REGULAR_FILE;

        Ok((i_dst.clone().into(), i_parent.clone().into()))
    }
}

impl vfs::InodeOps for InodeOps {
    fn create(&self, dst: &mut vfs::Inode, parent: &DEntry, path: Component) -> FSResult<()> {
        let mut i_vfs_parent = parent.inode_mut();

        let (i_dst, i_parent) = self.create_impl(dst, &mut i_vfs_parent, &path)?;

        // Update vfs inodes
        *dst = i_dst;
        *i_vfs_parent = i_parent;

        Ok(())
    }

    fn link(&self, src: &mut vfs::Inode, parent: &DEntry, path: Component) -> FSResult<()> {
        let mut i_vfs_parent = parent.inode_mut();

        let (i_dst, i_parent) = {
            let (i_dst, i_parent) = self.add_dir_entry(src, &mut i_vfs_parent, &path, false)?;
            (i_dst.clone().into(), i_parent.clone().into())
        };

        // Update vfs inodes
        *src = i_dst;
        *i_vfs_parent = i_parent;

        Ok(())
    }

    fn symlink(
        &self,
        dst: &mut vfs::Inode,
        src: &Path,
        parent: &DEntry,
        path: Component,
    ) -> FSResult<()> {
        let mut i_vfs_parent = parent.inode_mut();

        let (i_dst, i_parent) = {
            let (i_dst, i_parent) = self.add_dir_entry(dst, &mut i_vfs_parent, &path, true)?;

            // Set to symbolic link
            i_dst.mode = vfs::Mode::SYMBOLIC_LINK;

            let s_src = src.as_str();

            // Check if path is too long
            if s_src.len() > BLOCK_SIZE {
                return Err(vfs::FSError::BadPath);
            }

            // Set size to length of path
            i_dst.size = s_src.len() as u64;

            // Write path to first block
            let mut block = Box::new([0u8; BLOCK_SIZE]);
            block[..s_src.len()].copy_from_slice(s_src.as_bytes());
            let mut blocks = i_dst.blocks.write();
            blocks.push(block);

            (i_dst.clone().into(), i_parent.clone().into())
        };

        // Update vfs inodes
        *dst = i_dst;
        *i_vfs_parent = i_parent;

        Ok(())
    }

    fn unlink(&self, _dst: &mut vfs::Inode, _parent: &DEntry) -> FSResult<()> {
        Err(vfs::FSError::Unimplemented)
    }

    fn rename(
        &self,
        _src: &mut vfs::Inode,
        _src_p: &DEntry,
        _dst_p: &DEntry,
        _path: Component,
    ) -> FSResult<()> {
        Err(vfs::FSError::Unimplemented)
    }

    fn mkdir(&self, dst: &mut vfs::Inode, parent: &DEntry, path: Component) -> FSResult<()> {
        let mut i_vfs_parent = parent.inode_mut();

        let (i_dst, i_parent) = {
            let (i_dst, i_parent) = self.add_dir_entry(dst, &mut i_vfs_parent, &path, true)?;

            // Set to directory
            i_dst.mode = vfs::Mode::DIRECTORY;

            (i_dst.clone().into(), i_parent.clone().into())
        };

        // Update vfs inodes
        *dst = i_dst;
        *i_vfs_parent = i_parent;

        Ok(())
    }

    fn list<'b>(&self, inode: &'b vfs::Inode) -> FSResult<vfs::file_iter::FileIter<'b>> {
        let i: &Inode = inode
            .private
            .downcast_ref()
            .ok_or(vfs::FSError::WrongInode)?;

        if i.mode != vfs::Mode::DIRECTORY {
            return Err(vfs::FSError::NotDirectory);
        }

        let iter = DirIterator::new(i);
        Ok(vfs::file_iter::FileIter::new(inode, Box::new(iter)))
    }
}

const DIR_ENTRY_SIZE: usize = core::mem::size_of::<DirEntry>();

#[repr(C, packed)]
pub struct DirEntry {
    inode: u64,
    length: u8,
    name: [u8; 247],
}

assert_eq_size!(DirEntry, [u8; 256]);

impl DirEntry {
    const fn from_bytes(bytes: &[u8; DIR_ENTRY_SIZE]) -> &Self {
        // SAFETY: DirEntry is repr(C, packed) and has the same size as [u8; 256]
        unsafe { &*(bytes as *const [u8; DIR_ENTRY_SIZE]).cast::<Self>() }
    }

    fn from_bytes_mut(bytes: &mut [u8; DIR_ENTRY_SIZE]) -> &mut Self {
        // SAFETY: DirEntry is repr(C, packed) and has the same size as [u8; 256]
        unsafe { &mut *(bytes as *mut [u8; DIR_ENTRY_SIZE]).cast::<Self>() }
    }

    fn to_bytes(&self) -> [u8; DIR_ENTRY_SIZE] {
        let mut out = [0u8; DIR_ENTRY_SIZE];
        out[..8].copy_from_slice(&self.inode.to_ne_bytes());
        out[8] = self.length;
        out[9..].copy_from_slice(&self.name);
        out
    }
}

struct DirIterator<'a> {
    inode: &'a Inode,
    lock: RwLockReadGuard<'a, Vec<Box<[u8; BLOCK_SIZE]>>>,
    blkidx: usize,
    entryidx: usize,
}

impl<'a> DirIterator<'a> {
    fn new(inode: &'a Inode) -> Self {
        let lock = inode.blocks.read();
        Self {
            inode,
            lock,
            blkidx: 0,
            entryidx: 0,
        }
    }
}

impl Iterator for DirIterator<'_> {
    type Item = (PathBuf, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let Some(blk) = self.lock.get(self.blkidx) else {
            return None;
        };
        // Safety: BLK_SIZE (4096) is a multiple of DIR_ENTRY_SIZE (256)
        let chunks: &[[u8; DIR_ENTRY_SIZE]] =
            unsafe { blk.as_chunks_unchecked::<DIR_ENTRY_SIZE>() };

        let Some(entry) = chunks.get(self.entryidx).map(DirEntry::from_bytes) else {
            self.blkidx += 1;
            self.entryidx = 0;
            return self.next();
        };

        self.entryidx += 1;

        if entry.inode == 0 || entry.length == 0 {
            self.next()
        } else {
            let path =
                PathBuf::from(core::str::from_utf8(&entry.name[..entry.length as usize]).unwrap());
            Some((path, entry.inode))
        }
    }
}

impl FileIterator for DirIterator<'_> {}
