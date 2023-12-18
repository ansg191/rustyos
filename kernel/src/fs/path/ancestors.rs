use crate::fs::path::Path;

pub struct Ancestors<'a> {
    next: Option<&'a Path>,
}

impl<'a> Ancestors<'a> {
    pub const fn new(path: &'a Path) -> Self {
        Self { next: Some(path) }
    }
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Path;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        self.next = next.and_then(Path::parent);
        next
    }
}
