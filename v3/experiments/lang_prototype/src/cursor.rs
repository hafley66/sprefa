use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SprfPath(pub Vec<PathSeg>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSeg {
    Step(Arc<str>),
    ForkArm(usize),
    Param { name: Arc<str>, value: Option<Arc<str>> },
}

impl SprfPath {
    pub fn push(&mut self, seg: PathSeg) {
        self.0.push(seg);
    }
    pub fn clone_with(&self, seg: PathSeg) -> Self {
        let mut p = self.clone();
        p.push(seg);
        p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureSlot(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct Captures {
    inner: Vec<(CaptureSlot, super::lower::Scalar)>,
}

impl Default for Captures {
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}

impl Captures {
    pub fn get(&self, slot: CaptureSlot) -> Option<&super::lower::Scalar> {
        self.inner.iter().find(|(s, _)| *s == slot).map(|(_, v)| v)
    }
    pub fn insert(&mut self, slot: CaptureSlot, value: super::lower::Scalar) {
        if let Some(pos) = self.inner.iter().position(|(s, _)| *s == slot) {
            self.inner[pos] = (slot, value);
        } else {
            self.inner.push((slot, value));
        }
    }
    pub fn has(&self, slot: CaptureSlot) -> bool {
        self.inner.iter().any(|(s, _)| *s == slot)
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Clone, Debug)]
pub struct Cursor {
    pub path: SprfPath,
    pub content: Arc<[u8]>,
    pub byte_range: Range<usize>,
    pub captures: Captures,
    pub parent: Option<Arc<Cursor>>,
}

impl Cursor {
    pub fn new(path: SprfPath, content: Arc<[u8]>) -> Self {
        let len = content.len();
        Self {
            path,
            content,
            byte_range: 0..len,
            captures: Captures::default(),
            parent: None,
        }
    }

    pub fn active(&self) -> &[u8] {
        &self.content[self.byte_range.clone()]
    }

    pub fn narrow(&self, range: Range<usize>) -> Self {
        let mut c = self.clone();
        c.byte_range = range;
        c
    }

    pub fn with_capture(&self, slot: CaptureSlot, value: super::lower::Scalar) -> Self {
        let mut c = self.clone();
        c.captures.insert(slot, value);
        c
    }

    pub fn with_path(&self, path: SprfPath) -> Self {
        let mut c = self.clone();
        c.path = path;
        c
    }
}
