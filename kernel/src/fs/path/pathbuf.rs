use alloc::{collections::TryReserveError, string::String};
use core::{
    borrow::Borrow,
    fmt::Display,
    ops::{Deref, DerefMut},
};

use crate::fs::path::{Path, SEPERATOR};

/// An owned, mutable path (akin to [`String`]).
///
/// This type provides methods like [`push`] and [`set_extension`] that mutate
/// the path in place. It also implements [`Deref`] to [`Path`], meaning that
/// all methods on [`Path`] slices are available on `PathBuf` values as well.
///
/// [`push`]: PathBuf::push
/// [`set_extension`]: PathBuf::set_extension
///
/// More details about the overall approach can be found in
/// the [module documentation](self).
///
/// Will add custom allocator when #101551 is merged
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct PathBuf {
    inner: String,
}

impl PathBuf {
    // #[inline]
    // fn as_mut_vec(&mut self) -> &mut Vec<u8> {
    //     // SAFETY: `PathBuf` is layout-compatible with `String` which is layout-compatible with `Vec<u8>`
    //     unsafe { &mut *(self as *mut PathBuf as *mut Vec<u8>) }
    // }

    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self {
            inner: String::new(),
        }
    }

    #[must_use]
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: String::with_capacity(capacity),
        }
    }

    #[must_use]
    #[inline]
    pub fn as_path(&self) -> &Path {
        self
    }

    pub fn push<P: AsRef<Path>>(&mut self, path: P) {
        self._push(path.as_ref());
    }
    fn _push(&mut self, path: &Path) {
        // in general, a separator is needed if the rightmost byte is not a separator
        let need_sep = self.inner.chars().last().map_or(false, |c| c == SEPERATOR);

        if path.is_absolute() {
            // absolute `path` replaces `self`
            self.inner.clear();
        } else if path.has_root() {
            // `path` is a pure relative path
        } else if need_sep {
            self.inner.push(SEPERATOR);
        }

        self.inner.push_str(path.as_str());
    }

    pub fn pop(&mut self) -> bool {
        match self.parent().map(|p| p.as_str().len()) {
            Some(len) => {
                self.inner.truncate(len);
                true
            }
            None => false,
        }
    }

    pub fn set_file_name<S: AsRef<str>>(&mut self, file_name: S) {
        self._set_file_name(file_name.as_ref());
    }

    fn _set_file_name(&mut self, file_name: &str) {
        if self.file_name().is_some() {
            let popped = self.pop();
            debug_assert!(popped);
        }
        self.push(file_name);
    }

    pub fn set_extension<S: AsRef<str>>(&mut self, extension: S) -> bool {
        self._set_extension(extension.as_ref())
    }

    fn _set_extension(&mut self, extension: &str) -> bool {
        let file_stem = match self.file_stem() {
            None => return false,
            Some(f) => f,
        };

        // truncate until right after the file stem
        let end_file_stem = file_stem[file_stem.len()..].as_ptr() as usize;
        let start = self.inner.as_ptr() as usize;
        self.inner.truncate(end_file_stem.wrapping_sub(start));

        // add the new extension, if any
        let new = extension;
        if !new.is_empty() {
            self.inner.reserve_exact(new.len() + 1);
            self.inner.push('.');
            self.inner.push_str(new);
        }

        true
    }

    #[must_use]
    #[inline]
    pub fn as_mut_string(&mut self) -> &mut String {
        &mut self.inner
    }

    #[must_use = "`self` will be dropped if the result is not used"]
    #[inline]
    pub fn into_string(self) -> String {
        self.inner
    }

    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    #[inline]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.inner.try_reserve(additional)
    }

    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional);
    }

    #[inline]
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.inner.try_reserve_exact(additional)
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }

    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.inner.shrink_to(min_capacity);
    }
}

impl Deref for PathBuf {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        Path::new(&self.inner)
    }
}

impl DerefMut for PathBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Path::from_mut(&mut self.inner)
    }
}

impl AsRef<str> for PathBuf {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<Path> for PathBuf {
    fn as_ref(&self) -> &Path {
        self
    }
}

impl Borrow<Path> for PathBuf {
    fn borrow(&self) -> &Path {
        self
    }
}

impl<P: AsRef<Path>> Extend<P> for PathBuf {
    fn extend<T: IntoIterator<Item = P>>(&mut self, iter: T) {
        iter.into_iter().for_each(|p| self.push(p.as_ref()));
    }
}

impl From<&str> for PathBuf {
    fn from(s: &str) -> Self {
        Self {
            inner: String::from(s),
        }
    }
}

impl From<String> for PathBuf {
    fn from(s: String) -> Self {
        Self { inner: s }
    }
}

impl From<&Path> for PathBuf {
    fn from(p: &Path) -> Self {
        Self {
            inner: String::from(p.as_str()),
        }
    }
}

impl Display for PathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}
