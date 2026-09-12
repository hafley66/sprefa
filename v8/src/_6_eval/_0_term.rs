//! Terms are hash-consed into one arena. A `TermId` is the only thing a row
//! cell holds, so equality is `u32` equality and ordering is a walk of the
//! arena following SWI-Prolog standard order of terms.

use indexmap::IndexSet;
use std::cmp::Ordering;
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Sym(pub u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TermId(pub u32);

/// Proper lists are nested `'[|]'(Head, Tail)` compounds ending in the atom
/// `[]`, exactly as SWI-Prolog 7 stores them.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Term {
    Int(i64),
    Atom(Sym),
    Str(Sym),
    Compound(Sym, Vec<TermId>),
}

pub struct Universe {
    pub syms: IndexSet<String>,
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
            syms: IndexSet::new(),
            terms: IndexSet::new(),
            nil: Sym(0),
            cons: Sym(0),
        };
        u.nil = u.sym("[]");
        u.cons = u.sym("[|]");
        u
    }

    pub fn sym(&mut self, s: &str) -> Sym {
        if let Some(i) = self.syms.get_index_of(s) {
            return Sym(i as u32);
        }
        let (i, _) = self.syms.insert_full(s.to_string());
        Sym(i as u32)
    }

    pub fn sym_str(&self, s: Sym) -> &str {
        &self.syms[s.0 as usize]
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

    /// SWI-Prolog standard order as measured on 9.x:
    /// Number < String < `[]` < Atom < Compound. Compounds compare by arity,
    /// then name, then arguments left to right.
    pub fn cmp(&self, a: TermId, b: TermId) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        match (self.get(a), self.get(b)) {
            (Term::Int(x), Term::Int(y)) => x.cmp(y),
            (Term::Int(_), _) => Ordering::Less,
            (_, Term::Int(_)) => Ordering::Greater,
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
