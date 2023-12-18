use crate::fs::path::Path;

pub const SEPERATOR: char = '/';
pub const SEPERATOR_BYTE: u8 = b'/';
pub const SEPERATOR_STR: &str = "/";

pub const fn has_physical_root(s: &[u8]) -> bool {
    !s.is_empty() && s[0] == SEPERATOR_BYTE
}

const unsafe fn parse_single_component(comp: &[u8]) -> Option<Component> {
    match comp {
        b"." | b"" => None, // CurDir are handled in include_cur_dir
        b".." => Some(Component::ParentDir),
        _ => Some(Component::Normal(core::str::from_utf8_unchecked(comp))),
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Component<'a> {
    RootDir,
    CurDir,
    ParentDir,
    Normal(&'a str),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    StartDir, // / or .
    Body,     // /a/b/c
    Done,
}

#[derive(Debug, Clone)]
pub struct Components<'a> {
    path: &'a [u8],
    has_physical_root: bool,
    front: State,
    back: State,
}

impl<'a> Components<'a> {
    pub const fn new(path: &'a Path) -> Self {
        let has_physical_root = has_physical_root(path.as_str().as_bytes());
        Self {
            path: path.as_str().as_bytes(),
            has_physical_root,
            front: State::StartDir,
            back: State::Body,
        }
    }

    pub fn as_path(&self) -> &'a Path {
        let mut comps = self.clone();
        if comps.front == State::Body {
            comps.trim_left();
        }
        if comps.back == State::Body {
            comps.trim_right();
        }
        unsafe { Path::new(core::str::from_utf8_unchecked(comps.path)) }
    }

    #[inline]
    pub(super) const fn has_root(&self) -> bool {
        self.has_physical_root
    }

    // is the iteration complete?
    #[inline]
    fn finished(&self) -> bool {
        self.front == State::Done || self.back == State::Done || self.front > self.back
    }

    #[inline]
    fn len_before_body(&self) -> usize {
        let root = usize::from(self.front <= State::StartDir && self.has_physical_root);
        let cur_dir = usize::from(self.front <= State::StartDir && self.include_cur_dir());
        root + cur_dir
    }

    fn include_cur_dir(&self) -> bool {
        if self.has_root() {
            return false;
        }
        let mut iter = self.path.iter();
        match (iter.next(), iter.next()) {
            (Some(&b'.'), None) => true,
            (Some(&b'.'), Some(&b)) => b == SEPERATOR_BYTE,
            _ => false,
        }
    }

    /// parse a component from the left, saying how many bytes to consume to
    /// remove the component.
    fn parse_next_component(&self) -> (usize, Option<Component<'a>>) {
        debug_assert!(self.front == State::Body);
        let (extra, comp) = self
            .path
            .iter()
            .position(|b| *b == SEPERATOR_BYTE)
            .map_or((0, self.path), |i| (1, &self.path[..i]));

        // SAFETY: `comp` is a valid substring, since it is split on a separator.
        (comp.len() + extra, unsafe { parse_single_component(comp) })
    }

    /// parse a component from the right, saying how many bytes to consume to
    /// remove the component
    fn parse_next_component_back(&self) -> (usize, Option<Component<'a>>) {
        debug_assert!(self.back == State::Body);
        let start = self.len_before_body();
        let (extra, comp) = self.path[start..]
            .iter()
            .rposition(|b| *b == SEPERATOR_BYTE)
            .map_or((0, &self.path[start..]), |i| {
                (1, &self.path[start + i + 1..])
            });

        // SAFETY: `comp` is a valid substring, since it is split on a separator.
        (comp.len() + extra, unsafe { parse_single_component(comp) })
    }

    // trim away repeated separators (i.e., empty components) on the left
    fn trim_left(&mut self) {
        while !self.path.is_empty() {
            let (size, comp) = self.parse_next_component();
            if comp.is_some() {
                return;
            }
            self.path = &self.path[size..];
        }
    }

    // trim away repeated separators (i.e., empty components) on the right
    fn trim_right(&mut self) {
        while self.path.len() > self.len_before_body() {
            let (size, comp) = self.parse_next_component_back();
            if comp.is_some() {
                return;
            }
            self.path = &self.path[..self.path.len() - size];
        }
    }
}

impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.finished() {
            match self.front {
                State::StartDir => {
                    self.front = State::Body;
                    if self.has_physical_root {
                        debug_assert!(!self.path.is_empty());
                        self.path = &self.path[1..];
                        return Some(Component::RootDir);
                    } else if self.include_cur_dir() {
                        debug_assert!(!self.path.is_empty());
                        self.path = &self.path[1..];
                        return Some(Component::CurDir);
                    }
                }
                State::Body if !self.path.is_empty() => {
                    let (size, comp) = self.parse_next_component();
                    self.path = &self.path[size..];
                    if comp.is_some() {
                        return comp;
                    }
                }
                State::Body => {
                    self.front = State::Done;
                }
                State::Done => unreachable!(),
            }
        }
        None
    }
}

impl<'a> DoubleEndedIterator for Components<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while !self.finished() {
            match self.back {
                State::Body if self.path.len() > self.len_before_body() => {
                    let (size, comp) = self.parse_next_component_back();
                    self.path = &self.path[..self.path.len() - size];
                    if comp.is_some() {
                        return comp;
                    }
                }
                State::Body => {
                    self.back = State::StartDir;
                }
                State::StartDir => {
                    self.back = State::Done;
                    if self.has_physical_root {
                        self.path = &self.path[..self.path.len() - 1];
                        return Some(Component::RootDir);
                    } else if self.include_cur_dir() {
                        self.path = &self.path[..self.path.len() - 1];
                        return Some(Component::CurDir);
                    }
                }
                State::Done => unreachable!(),
            }
        }
        None
    }
}

impl AsRef<Path> for Component<'_> {
    fn as_ref(&self) -> &Path {
        match self {
            Component::RootDir => Path::new(SEPERATOR_STR),
            Component::CurDir => Path::new("."),
            Component::ParentDir => Path::new(".."),
            Component::Normal(s) => Path::new(s),
        }
    }
}

impl AsRef<Path> for Components<'_> {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}
