use alloc::boxed::Box;

use crate::fs::{path::PathBuf, vfs::Inode};

pub trait FileIterator: Iterator<Item = (PathBuf, u64)> {}

pub struct FileIter<'a> {
    inode: &'a Inode,
    iter: Box<dyn FileIterator + 'a>,
}

impl<'a> FileIter<'a> {
    pub fn new(inode: &'a Inode, iter: Box<dyn FileIterator + 'a>) -> Self {
        Self { inode, iter }
    }
}

impl Iterator for FileIter<'_> {
    type Item = (PathBuf, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl FileIterator for FileIter<'_> {}
