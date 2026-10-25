//! The program as written: names, not ids. The Rust builder below is the front; one call per lisp
//! form.
//!
//! ```text
//! (rel edge (from text) (to text))   rel("edge", [("from", Text), ("to", Text)])
//! (rule (reach a c) (edge a c))      rule(atom("reach", [v("a"), v("c")]), [pos("edge", [v("a"), v("c")])])
//! (not (blocked a))                  neg("blocked", [v("a")])
//! (add n 1 m)                        call(Add, [v("n"), int(1), v("m")])
//! ```

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Col {
    Text,
    Int,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelKind {
    Base,
    Derived,
    Request { service: String },
    Answer { service: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rel {
    pub name: String,
    pub cols: Vec<(String, Col)>,
    pub kind: RelKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    Var(String),
    Text(String),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Atom {
    pub rel: String,
    pub args: Vec<Term>,
}

/// `add(a, b, out)`: a, b bound Int, out bound or bound here. `lt(a, b)`: bound Int.
/// `neq(a, b)`: bound, one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Add,
    Lt,
    Neq,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Goal {
    Pos(Atom),
    Neg(Atom),
    Call(Builtin, Vec<Term>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rule {
    pub head: Atom,
    pub body: Vec<Goal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Program {
    pub rels: Vec<Rel>,
    pub rules: Vec<Rule>,
}

fn decl<'a>(name: &str, cols: impl IntoIterator<Item = (&'a str, Col)>, kind: RelKind) -> Rel {
    Rel {
        name: name.to_string(),
        cols: cols
            .into_iter()
            .map(|(col, ty)| (col.to_string(), ty))
            .collect(),
        kind,
    }
}

pub fn rel<'a>(name: &str, cols: impl IntoIterator<Item = (&'a str, Col)>) -> Rel {
    decl(name, cols, RelKind::Base)
}

pub fn derived<'a>(name: &str, cols: impl IntoIterator<Item = (&'a str, Col)>) -> Rel {
    decl(name, cols, RelKind::Derived)
}

pub fn request<'a>(
    service: &str,
    name: &str,
    cols: impl IntoIterator<Item = (&'a str, Col)>,
) -> Rel {
    decl(
        name,
        cols,
        RelKind::Request {
            service: service.to_string(),
        },
    )
}

/// `cols` start with the Request rel's cols, in order.
pub fn answer<'a>(
    service: &str,
    name: &str,
    cols: impl IntoIterator<Item = (&'a str, Col)>,
) -> Rel {
    decl(
        name,
        cols,
        RelKind::Answer {
            service: service.to_string(),
        },
    )
}

pub fn atom(rel: &str, args: impl IntoIterator<Item = Term>) -> Atom {
    Atom {
        rel: rel.to_string(),
        args: args.into_iter().collect(),
    }
}

pub fn rule(head: Atom, body: impl IntoIterator<Item = Goal>) -> Rule {
    Rule {
        head,
        body: body.into_iter().collect(),
    }
}

pub fn pos(rel: &str, args: impl IntoIterator<Item = Term>) -> Goal {
    Goal::Pos(atom(rel, args))
}

pub fn neg(rel: &str, args: impl IntoIterator<Item = Term>) -> Goal {
    Goal::Neg(atom(rel, args))
}

pub fn call(builtin: Builtin, args: impl IntoIterator<Item = Term>) -> Goal {
    Goal::Call(builtin, args.into_iter().collect())
}

pub fn v(name: &str) -> Term {
    Term::Var(name.to_string())
}

pub fn text(value: &str) -> Term {
    Term::Text(value.to_string())
}

pub fn int(value: i64) -> Term {
    Term::Int(value)
}
