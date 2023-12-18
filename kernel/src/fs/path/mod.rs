mod ancestors;
mod components;
mod pathbuf;

use alloc::borrow::{Cow, ToOwned};
use core::fmt::{Display, Formatter};

#[doc(no_inline)]
pub use self::{ancestors::*, components::*, pathbuf::PathBuf};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Path {
    inner: str,
}

impl Path {
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &Self {
        // SAFETY: `Path` is the same size as `str`
        unsafe { &*(s.as_ref() as *const str as *const Self) }
    }

    fn from_mut(s: &mut str) -> &mut Self {
        // SAFETY: `Path` is the same size as `str`
        unsafe { &mut *(s as *mut str as *mut Self) }
    }

    pub const fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn as_mut_str(&mut self) -> &mut str {
        &mut self.inner
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self)
    }

    pub const fn components(&self) -> Components {
        Components::new(self)
    }

    pub const fn has_root(&self) -> bool {
        self.components().has_root()
    }

    pub const fn is_absolute(&self) -> bool {
        self.has_root()
    }

    pub const fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    pub fn parent(&self) -> Option<&Self> {
        let mut comps = self.components();
        let comp = comps.next_back();
        comp.and_then(|p| match p {
            Component::CurDir | Component::ParentDir | Component::Normal(_) => {
                Some(comps.as_path())
            }
            Component::RootDir => None,
        })
    }

    pub const fn ancestors(&self) -> Ancestors {
        Ancestors::new(self)
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components().next_back().and_then(|p| match p {
            Component::Normal(p) => Some(p),
            _ => None,
        })
    }

    pub fn strip_prefix<P>(&self, base: P) -> Result<&Self, StripPrefixError>
    where
        P: AsRef<Self>,
    {
        self._strip_prefix(base.as_ref())
    }

    fn _strip_prefix(&self, base: &Self) -> Result<&Self, StripPrefixError> {
        iter_after(self.components(), base.components())
            .map(|c| c.as_path())
            .ok_or(StripPrefixError(()))
    }

    #[must_use]
    pub fn starts_with<P: AsRef<Self>>(&self, base: P) -> bool {
        self._starts_with(base.as_ref())
    }

    fn _starts_with(&self, base: &Self) -> bool {
        iter_after(self.components(), base.components()).is_some()
    }

    #[must_use]
    pub fn ends_with<P: AsRef<Self>>(&self, child: P) -> bool {
        self._ends_with(child.as_ref())
    }

    fn _ends_with(&self, child: &Self) -> bool {
        iter_after(self.components().rev(), child.components().rev()).is_some()
    }

    #[must_use]
    pub fn file_stem(&self) -> Option<&str> {
        self.file_name()
            .map(rsplit_file_at_dot)
            .and_then(|(before, after)| before.or(after))
    }

    #[must_use]
    pub fn file_prefix(&self) -> Option<&str> {
        self.file_name()
            .map(split_file_at_dot)
            .map(|(before, _after)| before)
    }

    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.file_name()
            .map(rsplit_file_at_dot)
            .and_then(|(before, after)| before.and(after))
    }

    #[must_use]
    pub fn join<P: AsRef<Self>>(&self, path: P) -> PathBuf {
        self._join(path.as_ref())
    }

    fn _join(&self, path: &Self) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.push(path);
        buf
    }

    #[must_use]
    pub fn with_file_name<S: AsRef<str>>(&self, file_name: S) -> PathBuf {
        self._with_file_name(file_name.as_ref())
    }

    fn _with_file_name(&self, file_name: &str) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.set_file_name(file_name);
        buf
    }

    pub fn with_extension<S: AsRef<str>>(&self, extension: S) -> PathBuf {
        self._with_extension(extension.as_ref())
    }

    fn _with_extension(&self, extension: &str) -> PathBuf {
        let self_len = self.as_str().len();
        let self_bytes = self.as_str();

        let (new_cap, slice) = self.extension().map_or_else(
            || (self_len + extension.len() + 1, self_bytes),
            |previous_extension| {
                let cap = self_len + extension.len() - previous_extension.len();
                (cap, &self_bytes[..self_len - previous_extension.len()])
            },
        );

        let mut new_path = PathBuf::with_capacity(new_cap);
        new_path.push(slice);
        new_path.set_extension(extension);
        new_path
    }
}

impl AsRef<str> for Path {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<Self> for Path {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl AsRef<Path> for Cow<'_, str> {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl ToOwned for Path {
    type Owned = PathBuf;

    fn to_owned(&self) -> PathBuf {
        self.to_path_buf()
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl PartialEq<str> for Path {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<Path> for str {
    fn eq(&self, other: &Path) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<PathBuf> for Path {
    fn eq(&self, other: &PathBuf) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Path> for PathBuf {
    fn eq(&self, other: &Path) -> bool {
        self.as_str() == other.as_str()
    }
}

// Iterate through `iter` while it matches `prefix`; return `None` if `prefix`
// is not a prefix of `iter`, otherwise return `Some(iter_after_prefix)` giving
// `iter` after having exhausted `prefix`.
fn iter_after<'a, 'b, I, J>(mut iter: I, mut prefix: J) -> Option<I>
where
    I: Iterator<Item = Component<'a>> + Clone,
    J: Iterator<Item = Component<'b>>,
{
    loop {
        let mut iter_next = iter.clone();
        match (iter_next.next(), prefix.next()) {
            (Some(ref x), Some(ref y)) if x == y => (),
            (Some(_) | None, Some(_)) => return None,
            (Some(_) | None, None) => return Some(iter),
        }
        iter = iter_next;
    }
}

// basic workhorse for splitting stem and extension
fn rsplit_file_at_dot(file: &str) -> (Option<&str>, Option<&str>) {
    if file == ".." {
        return (Some(file), None);
    }

    let mut iter = file.rsplitn(2, |b| b == '.');
    let after = iter.next();
    let before = iter.next();
    if before == Some("") {
        (Some(file), None)
    } else {
        (before, after)
    }
}

fn split_file_at_dot(file: &str) -> (&str, Option<&str>) {
    if file == ".." {
        return (file, None);
    }

    let i = match file[1..].chars().position(|b| b == '.') {
        Some(i) => i + 1,
        None => return (file, None),
    };
    let before = &file[..i];
    let after = &file[i + 1..];
    (before, Some(after))
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct StripPrefixError(());
