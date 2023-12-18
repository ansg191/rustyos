pub type FSResult<T, E = FSError> = Result<T, E>;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum FSError {
    /// Invalid path
    BadPath,
    /// File or directory not found
    NoEntry,
    /// File System is not mounted
    NoMount,
    /// Inode does not exist
    MissingInode,
    /// Inode belongs to a different superblock
    WrongInode,
    /// Inode is not a directory
    NotDirectory,
    /// File already exists
    Exists,
    /// Unimplemented
    Unimplemented,
    /// Not supported
    NotSupported,
}
