//! Path: addressable identity for any node in the tree-of-trees.
//!
//! A `Path` is an `Arc<[Box<dyn PathItem>]>`. Each item is opaque to the lib —
//! op crates extend the vocabulary by impl-ing `PathItem` for their own kinds
//! (GitRepo, GitRev, FsPath, ...). Lib ships four built-ins under `items`.
//!
//! Equality and hashing route through erased `dyn_eq` / `dyn_hash` so two
//! paths built with different concrete kinds compare correctly when their
//! erased state matches.

use std::any::Any;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub trait PathItem: Any + Debug + Send + Sync {
    fn render(&self) -> Cow<'_, str>;
    fn kind_tag(&self) -> &'static str;
    fn dyn_hash(&self, h: &mut dyn Hasher);
    fn dyn_eq(&self, other: &dyn PathItem) -> bool;
    fn as_any(&self) -> &dyn Any;
}

pub type Path = Arc<[Box<dyn PathItem>]>;

#[derive(Default)]
pub struct PathBuilder {
    items: Vec<Box<dyn PathItem>>,
}

impl PathBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn with_capacity(n: usize) -> Self { Self { items: Vec::with_capacity(n) } }

    pub fn push<I: PathItem>(&mut self, item: I) -> &mut Self {
        self.items.push(Box::new(item));
        self
    }

    pub fn push_boxed(&mut self, item: Box<dyn PathItem>) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn extend_from(&mut self, path: &Path) -> &mut Self {
        for it in path.iter() {
            self.items.push(clone_item(it.as_ref()));
        }
        self
    }

    pub fn len(&self) -> usize { self.items.len() }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }

    pub fn build(self) -> Path {
        Arc::from(self.items.into_boxed_slice())
    }
}

/// Best-effort clone of a boxed PathItem. Concrete types must be in the
/// `items` registry below to clone; otherwise we panic. Lib-shipped items all
/// support clone. Op crates that want clonable custom items must register them
/// (future hook; for now they impl Clone via the trait method on themselves
/// and route through `clone_item` by adding their kind_tag to `clone_item`).
fn clone_item(item: &dyn PathItem) -> Box<dyn PathItem> {
    match item.kind_tag() {
        "Index" => {
            let v = item.as_any().downcast_ref::<items::Index>().expect("kind tag mismatch");
            Box::new(v.clone())
        }
        "Name" => {
            let v = item.as_any().downcast_ref::<items::Name>().expect("kind tag mismatch");
            Box::new(v.clone())
        }
        "Field" => {
            let v = item.as_any().downcast_ref::<items::Field>().expect("kind tag mismatch");
            Box::new(v.clone())
        }
        "Body" => Box::new(items::Body),
        other => panic!(
            "PathBuilder::extend_from: unknown PathItem kind_tag {other:?}; \
             extend the clone_item registry or build the path from primitives"
        ),
    }
}

// ─── Path operations ───────────────────────────────────────────────────────

pub fn parent(p: &Path) -> Option<Path> {
    if p.is_empty() { return None; }
    let mut b = PathBuilder::with_capacity(p.len() - 1);
    for it in &p[..p.len() - 1] {
        b.push_boxed(clone_item(it.as_ref()));
    }
    Some(b.build())
}

pub fn is_prefix_of(prefix: &Path, p: &Path) -> bool {
    if prefix.len() > p.len() { return false; }
    for (a, b) in prefix.iter().zip(p.iter()) {
        if !a.dyn_eq(b.as_ref()) { return false; }
    }
    true
}

pub fn strip_prefix(prefix: &Path, p: &Path) -> Option<Path> {
    if !is_prefix_of(prefix, p) { return None; }
    let mut b = PathBuilder::with_capacity(p.len() - prefix.len());
    for it in &p[prefix.len()..] {
        b.push_boxed(clone_item(it.as_ref()));
    }
    Some(b.build())
}

pub fn render_uri(p: &Path, scheme: &str, sep: &str) -> String {
    let mut out = String::with_capacity(scheme.len() + p.len() * 8);
    out.push_str(scheme);
    for (i, it) in p.iter().enumerate() {
        if i > 0 || !scheme.is_empty() {
            out.push_str(sep);
        }
        out.push_str(&it.render());
    }
    out
}

// ─── Path equality / hash adapter ──────────────────────────────────────────
// `Path` is `Arc<[Box<dyn PathItem>]>`. Slice equality on Box<dyn Trait>
// compares pointers, not contents. Use these wrappers for correct semantics.

#[derive(Clone)]
pub struct PathKey(pub Path);

impl PartialEq for PathKey {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() { return false; }
        self.0.iter().zip(other.0.iter())
            .all(|(a, b)| a.dyn_eq(b.as_ref()))
    }
}
impl Eq for PathKey {}

impl Hash for PathKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for it in self.0.iter() {
            state.write(it.kind_tag().as_bytes());
            it.dyn_hash(state);
        }
    }
}

impl Debug for PathKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.iter().map(|it| it.render())).finish()
    }
}

// ─── TS-backed PathItem emitter helper ─────────────────────────────────────

/// Walk from `tree.root_node()` down to the deepest descendant containing
/// `body_byte`, emitting one `items::Field { kind_id, idx }` per step.
/// Helper for TS-backed DSLs implementing `Compiled::emit_path_items`.
pub fn ts_default_emit(
    tree: &tree_sitter::Tree,
    body_byte: usize,
    builder: &mut PathBuilder,
) {
    let root = tree.root_node();
    let target = root.descendant_for_byte_range(body_byte, body_byte).unwrap_or(root);
    if target.id() == root.id() { return; }
    let mut chain: Vec<items::Field> = Vec::new();
    let mut cursor = target;
    while let Some(parent) = cursor.parent() {
        let kind_id = cursor.kind_id();
        let mut idx: u16 = 0;
        let mut walker = parent.walk();
        for (i, child) in parent.children(&mut walker).enumerate() {
            if child.id() == cursor.id() {
                idx = u16::try_from(i).unwrap_or(u16::MAX);
                break;
            }
        }
        chain.push(items::Field { kind_id, idx });
        if parent.id() == root.id() { break; }
        cursor = parent;
    }
    chain.reverse();
    for f in chain { builder.push(f); }
}

// ─── Built-in items ────────────────────────────────────────────────────────

pub mod items {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Index(pub u16);

    impl PathItem for Index {
        fn render(&self) -> Cow<'_, str> { Cow::Owned(self.0.to_string()) }
        fn kind_tag(&self) -> &'static str { "Index" }
        fn dyn_hash(&self, h: &mut dyn Hasher) { h.write_u16(self.0); }
        fn dyn_eq(&self, other: &dyn PathItem) -> bool {
            other.as_any().downcast_ref::<Index>().map_or(false, |o| o.0 == self.0)
        }
        fn as_any(&self) -> &dyn Any { self }
    }

    #[derive(Clone, Debug)]
    pub struct Name(pub Arc<str>);

    impl Name {
        pub fn new(s: impl Into<Arc<str>>) -> Self { Self(s.into()) }
    }

    impl PathItem for Name {
        fn render(&self) -> Cow<'_, str> { Cow::Borrowed(&self.0) }
        fn kind_tag(&self) -> &'static str { "Name" }
        fn dyn_hash(&self, h: &mut dyn Hasher) { h.write(self.0.as_bytes()); }
        fn dyn_eq(&self, other: &dyn PathItem) -> bool {
            other.as_any().downcast_ref::<Name>().map_or(false, |o| *o.0 == *self.0)
        }
        fn as_any(&self) -> &dyn Any { self }
    }

    /// Tree-sitter field+index. Default segment kind for TS-backed DSLs.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Field { pub kind_id: u16, pub idx: u16 }

    impl PathItem for Field {
        fn render(&self) -> Cow<'_, str> {
            Cow::Owned(format!("{}[{}]", self.kind_id, self.idx))
        }
        fn kind_tag(&self) -> &'static str { "Field" }
        fn dyn_hash(&self, h: &mut dyn Hasher) {
            h.write_u16(self.kind_id);
            h.write_u16(self.idx);
        }
        fn dyn_eq(&self, other: &dyn PathItem) -> bool {
            other.as_any().downcast_ref::<Field>()
                .map_or(false, |o| o.kind_id == self.kind_id && o.idx == self.idx)
        }
        fn as_any(&self) -> &dyn Any { self }
    }

    /// `$` — cross from container into content (e.g. file → file body).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Body;

    impl PathItem for Body {
        fn render(&self) -> Cow<'_, str> { Cow::Borrowed("$") }
        fn kind_tag(&self) -> &'static str { "Body" }
        fn dyn_hash(&self, h: &mut dyn Hasher) { h.write_u8(0); }
        fn dyn_eq(&self, other: &dyn PathItem) -> bool {
            other.as_any().downcast_ref::<Body>().is_some()
        }
        fn as_any(&self) -> &dyn Any { self }
    }
}

#[cfg(test)]
mod tests {
    use super::items::*;
    use super::*;
    use std::collections::HashMap;

    fn p<I: IntoIterator<Item = Box<dyn PathItem>>>(items: I) -> Path {
        let mut b = PathBuilder::new();
        for it in items { b.push_boxed(it); }
        b.build()
    }

    #[test]
    fn pathkey_eq_and_hash_routes_through_erased_methods() {
        let a = p([Box::new(Index(0)) as _, Box::new(Name::new("x")) as _]);
        let b = p([Box::new(Index(0)) as _, Box::new(Name::new("x")) as _]);
        assert_eq!(PathKey(a.clone()), PathKey(b.clone()));

        let mut m: HashMap<PathKey, i32> = HashMap::new();
        m.insert(PathKey(a), 7);
        assert_eq!(m.get(&PathKey(b)), Some(&7));
    }

    #[test]
    fn parent_strips_one() {
        let path = p([Box::new(Index(0)) as _, Box::new(Body) as _, Box::new(Name::new("k")) as _]);
        let par = parent(&path).unwrap();
        assert_eq!(par.len(), 2);
        let empty: Vec<Box<dyn PathItem>> = vec![];
        assert!(parent(&p(empty)).is_none());
    }

    #[test]
    fn prefix_and_strip() {
        let pre = p([Box::new(Index(0)) as _, Box::new(Body) as _]);
        let full = p([Box::new(Index(0)) as _, Box::new(Body) as _, Box::new(Name::new("k")) as _]);
        assert!(is_prefix_of(&pre, &full));
        let rest = strip_prefix(&pre, &full).unwrap();
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn render_uri_concatenates_with_separator() {
        let path = p([
            Box::new(Name::new("git://abc")) as _,
            Box::new(Name::new("@main")) as _,
            Box::new(Body) as _,
            Box::new(Name::new("rs_let")) as _,
        ]);
        assert_eq!(render_uri(&path, "", "/"), "git://abc/@main/$/rs_let");
    }

    #[test]
    fn cross_kind_inequality() {
        let a = p([Box::new(Index(0)) as _]);
        let b = p([Box::new(Name::new("0")) as _]);
        assert_ne!(PathKey(a), PathKey(b));
    }
}
