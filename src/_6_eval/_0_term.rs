//! Terms are hash-consed into one arena. A `TermId` is the only thing a row
//! cell holds, so equality is `u32` equality and ordering is a walk of the
//! arena following SWI-Prolog standard order of terms.

use hashbrown::HashTable;
use indexmap::IndexSet;
use lasso::{Key, Rodeo, Spur};
use ordered_float::OrderedFloat;
use std::cmp::Ordering;
use std::fmt;
use std::hash::BuildHasher;
use std::sync::Arc;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Sym(pub u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TermId(pub u32);

/// Proper lists are nested `'[|]'(Head, Tail)` compounds ending in the atom
/// `[]`, exactly as SWI-Prolog 7 stores them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Term {
    Int(i64),
    Float(OrderedFloat<f64>),
    Bool(bool),
    Atom(Sym),
    Str(Sym),
    Compound(Sym, Vec<TermId>),
}

/// One loaded source text. A `Sym` can name a byte range of it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Blob(pub u32);

/// Where the bytes of one `Sym` live.
#[derive(Copy, Clone, Debug)]
enum Slot {
    /// Interned whole in the arena.
    Arena(Spur),
    /// A byte range of a loaded blob, never copied.
    Span { blob: u32, start: u32, end: u32 },
}

/// Every name and text value, each stored once. Text minted by the engine
/// lives in the `lasso` arena; text read from a loaded blob stays a span of
/// that blob. A `Sym` is dense and allocation-ordered across both, and one
/// content has one `Sym` whichever way it arrived.
pub struct Syms {
    arena: Rodeo,
    /// Spur index to `Sym`.
    arena_syms: Vec<u32>,
    slots: Vec<Slot>,
    blobs: Vec<Arc<str>>,
    /// Blobs by content: a text read again reuses its blob.
    blob_ids: std::collections::HashMap<Arc<str>, Blob>,
    /// Span `Sym`s by content.
    spans: HashTable<u32>,
    hasher: hashbrown::DefaultHashBuilder,
}

impl Default for Syms {
    fn default() -> Self {
        Syms {
            arena: Rodeo::new(),
            arena_syms: Vec::new(),
            slots: Vec::new(),
            blobs: Vec::new(),
            blob_ids: std::collections::HashMap::new(),
            spans: HashTable::new(),
            hasher: hashbrown::DefaultHashBuilder::default(),
        }
    }
}

impl Syms {
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn text(&self, slot: Slot) -> &str {
        match slot {
            Slot::Arena(spur) => self.arena.resolve(&spur),
            Slot::Span { blob, start, end } => {
                &self.blobs[blob as usize][start as usize..end as usize]
            }
        }
    }

    /// The text of `Sym` number `index`.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.slots.get(index).map(|slot| self.text(*slot))
    }

    /// The `Sym` number holding `text`, minting nothing.
    pub fn get_index_of(&self, text: &str) -> Option<usize> {
        if let Some(spur) = self.arena.get(text) {
            return Some(self.arena_syms[spur.into_usize()] as usize);
        }
        let hash = self.hasher.hash_one(text);
        self.spans
            .find(hash, |sym| self.text(self.slots[*sym as usize]) == text)
            .map(|sym| *sym as usize)
    }

    fn intern(&mut self, text: &str) -> Sym {
        if let Some(index) = self.get_index_of(text) {
            return Sym(index as u32);
        }
        let spur = self.arena.get_or_intern(text);
        let sym = self.slots.len() as u32;
        self.slots.push(Slot::Arena(spur));
        debug_assert_eq!(spur.into_usize(), self.arena_syms.len());
        self.arena_syms.push(sym);
        Sym(sym)
    }

    pub fn blob(&mut self, text: Arc<str>) -> Blob {
        if let Some(blob) = self.blob_ids.get(&text) {
            return *blob;
        }
        let blob = Blob(self.blobs.len() as u32);
        self.blobs.push(text.clone());
        self.blob_ids.insert(text, blob);
        blob
    }

    /// Bytes `start..end` of `blob`, on char boundaries.
    fn intern_span(&mut self, blob: Blob, start: usize, end: usize) -> Sym {
        let text = &self.blobs[blob.0 as usize][start..end];
        if let Some(index) = self.get_index_of(text) {
            return Sym(index as u32);
        }
        let hash = self.hasher.hash_one(text);
        let sym = self.slots.len() as u32;
        self.slots.push(Slot::Span {
            blob: blob.0,
            start: start as u32,
            end: end as u32,
        });
        let Syms {
            spans,
            slots,
            blobs,
            arena,
            hasher,
            ..
        } = self;
        let text_of = |sym: &u32| match slots[*sym as usize] {
            Slot::Arena(spur) => arena.resolve(&spur),
            Slot::Span { blob, start, end } => &blobs[blob as usize][start as usize..end as usize],
        };
        spans.insert_unique(hash, sym, |sym| hasher.hash_one(text_of(sym)));
        Sym(sym)
    }
}

impl std::ops::Index<usize> for Syms {
    type Output = str;

    fn index(&self, index: usize) -> &str {
        self.text(self.slots[index])
    }
}

pub struct Universe {
    pub syms: Syms,
    pub terms: IndexSet<Term>,
    pub nil: Sym,
    pub cons: Sym,
}

impl Default for Universe {
    fn default() -> Self {
        Self::new()
    }
}

impl Universe {
    pub fn new() -> Self {
        let mut u = Universe {
            syms: Syms::default(),
            terms: IndexSet::new(),
            nil: Sym(0),
            cons: Sym(0),
        };
        u.nil = u.sym("[]");
        u.cons = u.sym("[|]");
        u
    }

    pub fn sym(&mut self, s: &str) -> Sym {
        self.syms.intern(s)
    }

    pub fn sym_str(&self, s: Sym) -> &str {
        &self.syms[s.0 as usize]
    }

    /// Registers one loaded text; spans of it intern without a copy.
    pub fn blob(&mut self, text: impl Into<Arc<str>>) -> Blob {
        self.syms.blob(text.into())
    }

    /// The name or text at bytes `start..end` of `blob`.
    pub fn sym_span(&mut self, blob: Blob, start: usize, end: usize) -> Sym {
        self.syms.intern_span(blob, start, end)
    }

    pub fn string_span(&mut self, blob: Blob, start: usize, end: usize) -> TermId {
        let s = self.sym_span(blob, start, end);
        self.intern(Term::Str(s))
    }

    /// The whole of one loaded text, sharing its allocation with the loader:
    /// a span over it unless that text already has a `Sym`.
    pub fn blob_string(&mut self, text: Arc<str>) -> TermId {
        if let Some(index) = self.syms.get_index_of(&text) {
            return self.intern(Term::Str(Sym(index as u32)));
        }
        let end = text.len();
        let blob = self.blob(text);
        self.string_span(blob, 0, end)
    }

    pub fn atom_span(&mut self, blob: Blob, start: usize, end: usize) -> TermId {
        let s = self.sym_span(blob, start, end);
        self.intern(Term::Atom(s))
    }

    pub fn intern(&mut self, t: Term) -> TermId {
        let (i, _) = self.terms.insert_full(t);
        TermId(i as u32)
    }

    pub fn get(&self, id: TermId) -> &Term {
        &self.terms[id.0 as usize]
    }

    pub fn int(&mut self, n: i64) -> TermId {
        self.intern(Term::Int(n))
    }

    pub fn float(&mut self, f: f64) -> TermId {
        self.intern(Term::Float(OrderedFloat(f)))
    }

    pub fn boolean(&mut self, b: bool) -> TermId {
        self.intern(Term::Bool(b))
    }

    pub fn atom(&mut self, name: &str) -> TermId {
        let s = self.sym(name);
        self.intern(Term::Atom(s))
    }

    pub fn string(&mut self, text: &str) -> TermId {
        let s = self.sym(text);
        self.intern(Term::Str(s))
    }

    pub fn compound(&mut self, name: &str, args: Vec<TermId>) -> TermId {
        let s = self.sym(name);
        self.intern(Term::Compound(s, args))
    }

    pub fn empty_list(&mut self) -> TermId {
        let nil = self.nil;
        self.intern(Term::Atom(nil))
    }

    pub fn list(&mut self, items: &[TermId]) -> TermId {
        let mut tail = self.empty_list();
        for &item in items.iter().rev() {
            let cons = self.cons;
            tail = self.intern(Term::Compound(cons, vec![item, tail]));
        }
        tail
    }

    /// `Some(items)` for a proper list, `None` otherwise.
    pub fn as_list(&self, mut id: TermId) -> Option<Vec<TermId>> {
        let mut out = Vec::new();
        loop {
            match self.get(id) {
                Term::Atom(s) if *s == self.nil => return Some(out),
                Term::Compound(s, args) if *s == self.cons && args.len() == 2 => {
                    out.push(args[0]);
                    id = args[1];
                }
                _ => return None,
            }
        }
    }

    pub fn as_int(&self, id: TermId) -> Option<i64> {
        match self.get(id) {
            Term::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// `name(arg)` with exactly one argument.
    pub fn unary(&self, id: TermId, name: &str) -> Option<TermId> {
        match self.get(id) {
            Term::Compound(s, args) if args.len() == 1 && self.sym_str(*s) == name => Some(args[0]),
            _ => None,
        }
    }

    /// Compound name and args, or an atom as a name with no args.
    pub fn functor_or_atom(&self, id: TermId) -> Option<(&str, &[TermId])> {
        match self.get(id) {
            Term::Compound(s, args) => Some((self.sym_str(*s), args)),
            Term::Atom(s) => Some((self.sym_str(*s), &[])),
            _ => None,
        }
    }

    pub fn functor(&self, id: TermId) -> Option<(&str, &[TermId])> {
        match self.get(id) {
            Term::Compound(s, args) => Some((self.sym_str(*s), args)),
            _ => None,
        }
    }

    /// The arguments of `name/N` copied into a fixed array, so the caller can
    /// go on mutating the universe without a heap row per read.
    pub fn args<const N: usize>(&self, id: TermId, name: &str) -> Option<[TermId; N]> {
        let (functor, args) = self.functor(id)?;
        if functor != name || args.len() != N {
            return None;
        }
        Some(std::array::from_fn(|i| args[i]))
    }

    /// SWI-Prolog standard order as measured on 9.x: Number < Bool < String <
    /// `[]` < Atom < Compound. Numbers compare by value with Int before Float on
    /// a tie; Bool is `false` then `true`. Compounds compare by arity, then
    /// name, then arguments left to right.
    pub fn cmp(&self, a: TermId, b: TermId) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        match (self.get(a), self.get(b)) {
            (Term::Int(x), Term::Int(y)) => x.cmp(y),
            (Term::Float(x), Term::Float(y)) => x.cmp(y),
            (Term::Int(x), Term::Float(y)) => OrderedFloat(*x as f64).cmp(y).then(Ordering::Less),
            (Term::Float(x), Term::Int(y)) => {
                x.cmp(&OrderedFloat(*y as f64)).then(Ordering::Greater)
            }
            (Term::Int(_) | Term::Float(_), _) => Ordering::Less,
            (_, Term::Int(_) | Term::Float(_)) => Ordering::Greater,
            (Term::Bool(x), Term::Bool(y)) => x.cmp(y),
            (Term::Bool(_), _) => Ordering::Less,
            (_, Term::Bool(_)) => Ordering::Greater,
            (Term::Str(x), Term::Str(y)) => self.sym_str(*x).cmp(self.sym_str(*y)),
            (Term::Str(_), _) => Ordering::Less,
            (_, Term::Str(_)) => Ordering::Greater,
            (Term::Atom(x), Term::Atom(y)) => {
                if *x == self.nil {
                    Ordering::Less
                } else if *y == self.nil {
                    Ordering::Greater
                } else {
                    self.sym_str(*x).cmp(self.sym_str(*y))
                }
            }
            (Term::Atom(_), _) => Ordering::Less,
            (_, Term::Atom(_)) => Ordering::Greater,
            (Term::Compound(fx, ax), Term::Compound(fy, ay)) => ax
                .len()
                .cmp(&ay.len())
                .then_with(|| self.sym_str(*fx).cmp(self.sym_str(*fy)))
                .then_with(|| {
                    for (x, y) in ax.iter().zip(ay.iter()) {
                        let o = self.cmp(*x, *y);
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    Ordering::Equal
                }),
        }
    }

    pub fn cmp_rows(&self, a: &[TermId], b: &[TermId]) -> Ordering {
        for (x, y) in a.iter().zip(b.iter()) {
            let o = self.cmp(*x, *y);
            if o != Ordering::Equal {
                return o;
            }
        }
        a.len().cmp(&b.len())
    }

    pub fn display(&self, id: TermId) -> TermDisplay<'_> {
        TermDisplay { u: self, id }
    }
}

pub struct TermDisplay<'a> {
    u: &'a Universe,
    id: TermId,
}

fn atom_needs_quotes(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => !chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
        _ => s != "[]",
    }
}

impl fmt::Display for TermDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let u = self.u;
        if let Some(items) = u.as_list(self.id) {
            if !items.is_empty() {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", u.display(*item))?;
                }
                return write!(f, "]");
            }
        }
        match u.get(self.id) {
            Term::Int(n) => write!(f, "{n}"),
            Term::Float(x) => write!(f, "{:?}", x.0),
            Term::Bool(b) => write!(f, "{b}"),
            Term::Atom(s) => {
                let name = u.sym_str(*s);
                if atom_needs_quotes(name) {
                    write!(f, "'{}'", name.replace('\'', "\\'"))
                } else {
                    write!(f, "{name}")
                }
            }
            Term::Str(s) => write!(f, "\"{}\"", u.sym_str(*s).replace('"', "\\\"")),
            Term::Compound(s, args) => {
                let name = u.sym_str(*s);
                if atom_needs_quotes(name) {
                    write!(f, "'{name}'(")?;
                } else {
                    write!(f, "{name}(")?;
                }
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", u.display(*a))?;
                }
                write!(f, ")")
            }
        }
    }
}
