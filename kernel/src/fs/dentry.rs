use alloc::sync::Arc;
use core::{
    fmt::{Debug, Formatter},
    iter::Peekable,
    sync::atomic::{AtomicU64, Ordering},
};

use hashbrown::HashMap;
use spin::{
    lock_api::{RwLock, RwLockReadGuard, RwLockWriteGuard},
    Lazy,
};

use crate::{
    fs::{
        path::{Component, Path, PathBuf},
        vfs,
        vfs::{FSError, FSResult, FileSystem, Inode},
        MOUNTS,
    },
    time::TICKS,
};

const CACHE_SIZE: usize = 0x8000 / core::mem::size_of::<DEntry>();

pub static DIR_CACHE: Lazy<DirectoryCache> = Lazy::new(DirectoryCache::new);

type Entries = HashMap<PathBuf, (DEntry, AtomicU64)>;

pub struct DirectoryCache {
    entries: RwLock<Entries>,
}
impl DirectoryCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::with_capacity(CACHE_SIZE)),
        }
    }

    pub fn mount(&self, dentry: DEntry) {
        let mut lock = self.entries.write();
        insert_entry(&mut lock, dentry);
    }

    /// Reloads dentry from disk
    pub fn reload(&self, dentry: &DEntry) -> FSResult<()> {
        let inode = {
            let fs = dentry.fs();
            let lock = fs.superblock();
            let sb = lock.read();
            sb.get_inode(dentry.inode().num)?
        };

        let mut lock = self.entries.write();
        if let Some(inode) = inode {
            let mut i = dentry.inode_mut();
            *i = inode;
        } else {
            lock.remove(&*dentry.name());
        }
        Ok(())
    }

    fn get_opt(&self, path: &Path) -> Option<DEntry> {
        self.entries.read().get(path).map(|entry| {
            entry.1.store(TICKS.get(), Ordering::SeqCst);
            entry.0.clone()
        })
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> FSResult<DEntry> {
        self._get(path.as_ref())
    }
    fn _get(&self, path: &Path) -> FSResult<DEntry> {
        // Fast path, entry already cached
        if let Some(entry) = self.get_opt(path) {
            return Ok(entry);
        }

        // Slow path, entry not cached
        for parent in path.ancestors() {
            if let Some(entry) = self.get_opt(parent) {
                let remaining = path.strip_prefix(parent).unwrap().components();

                let mut lock = self.entries.write();
                return fill_path(&mut lock, parent, entry, remaining.peekable());
            }
        }

        // Entry not found, that means there is no disk mounted at root
        panic!("No disk mounted at root");
    }

    pub fn delete(&self, path: &Path) {
        self.entries.write().remove(path);
    }
    pub fn delete_inode(&self, fs: &dyn FileSystem, inode: &Inode) {
        self.entries.write().retain(|_, entry| {
            entry.0.fs().name() != fs.name() || entry.0.inode().num != inode.num
        });
    }
    pub fn unmount(&self, fs: &dyn FileSystem) {
        self.entries
            .write()
            .retain(|_, entry| entry.0.fs().name() != fs.name());
    }
}

/// Fill the cache with the entries from `cached_parent` to path
fn fill_path<'a, C, P>(
    cache: &mut Entries,
    parent: P,
    pdentry: DEntry,
    mut comps: Peekable<C>,
) -> FSResult<DEntry>
where
    C: Iterator<Item = Component<'a>>,
    P: Into<PathBuf>,
{
    let Some(comp) = comps.next() else {
        return Ok(pdentry);
    };

    let inode = pdentry.inode();

    // If entry is not a dir and there are more components, fail
    if !inode.is_dir() && comps.peek().is_some() {
        return Err(FSError::NoEntry);
    }

    // Retrieve the directory entries
    let dir_entries = inode.ops().list(&inode)?;

    // Search for the entry in the directory
    for (path, inode_n) in dir_entries {
        if comp.as_ref() != &*path {
            continue;
        }

        let mut new_path = parent.into();
        new_path.push(path);

        // Insert the entry into the cache
        let entry = {
            let fs = pdentry.fs();
            let l_sb = fs.superblock();
            let sb = l_sb.read();
            DEntry::new(
                new_path.clone(),
                sb.get_inode(inode_n)?.ok_or(FSError::MissingInode)?,
                pdentry.fs_arc(),
            )
        };

        insert_entry(cache, entry.clone());

        return fill_path(cache, new_path, entry, comps);
    }

    // Entry not found
    Err(FSError::NoEntry)
}

/// Insert a new entry into the cache
///
/// Evicts the least recently used entry if the cache is full
fn insert_entry(entries: &mut Entries, entry: DEntry) {
    if entries.len() >= CACHE_SIZE {
        evict_entry(entries);
    }

    let name = entry.name().to_path_buf();

    entries.insert(name, (entry, AtomicU64::new(TICKS.get())));
}

fn evict_entry(entries: &mut Entries) {
    let mut lru = None;
    let mut lru_time = u64::MAX;

    for (path, entry) in entries.iter() {
        if MOUNTS.is_mount_path(path) {
            // Don't evict entries for root mount points
            continue;
        }

        let last_access = entry.1.load(Ordering::SeqCst);
        if last_access < lru_time {
            lru = Some(path.clone());
            lru_time = last_access;
        }
    }

    if let Some(lru) = lru {
        entries.remove(&lru);
    }
}

#[derive(Debug, Clone)]
pub struct DEntry(Arc<RwLock<DEntryInner>>);

struct DEntryInner {
    /// Cached path
    name: PathBuf,
    /// Cached inode
    inode: Inode,
    /// Filesystem key in the mount table
    fs: Arc<dyn vfs::FileSystem + Send + Sync>,
}

pub type MappedReadGuard<'a, T> = lock_api::MappedRwLockReadGuard<'a, spin::RwLock<()>, T>;
pub type MappedWriteGuard<'a, T> = lock_api::MappedRwLockWriteGuard<'a, spin::RwLock<()>, T>;

impl DEntry {
    pub fn new<P: Into<PathBuf>>(
        name: P,
        inode: Inode,
        fs: Arc<dyn vfs::FileSystem + Send + Sync>,
    ) -> Self {
        Self(Arc::new(RwLock::new(DEntryInner {
            name: name.into(),
            inode,
            fs,
        })))
    }

    pub fn reload(&self) -> FSResult<()> {
        DIR_CACHE.reload(self)
    }

    pub fn name(&self) -> MappedReadGuard<Path> {
        RwLockReadGuard::map(self.0.read(), |inner| &*inner.name)
    }
    pub fn inode(&self) -> MappedReadGuard<Inode> {
        RwLockReadGuard::map(self.0.read(), |inner| &inner.inode)
    }
    pub fn inode_mut(&self) -> MappedWriteGuard<Inode> {
        RwLockWriteGuard::map(self.0.write(), |inner| &mut inner.inode)
    }
    pub fn fs(&self) -> MappedReadGuard<dyn vfs::FileSystem + Send + Sync> {
        RwLockReadGuard::map(self.0.read(), |inner| &*inner.fs)
    }
    pub fn fs_arc(&self) -> Arc<dyn vfs::FileSystem + Send + Sync> {
        self.0.read().fs.clone()
    }
}

impl Debug for DEntryInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DEntry")
            .field("name", &self.name)
            .field("inode", &self.inode)
            .field("fs", &self.fs.name())
            .finish()
    }
}
