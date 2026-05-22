use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type { Text, Int, Path, File, Dir }

impl Type {
    pub fn sql(self) -> &'static str {
        match self { Type::Int => "INTEGER", _ => "TEXT" }
    }
    pub fn parse(s: &str) -> Option<Type> {
        Some(match s {
            "text" => Type::Text,
            "int" => Type::Int,
            "path" => Type::Path,
            "file" => Type::File,
            "dir" => Type::Dir,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub enum Value { Text(String), Int(i64) }

impl Value {
    pub fn as_str(&self) -> String {
        match self { Value::Text(s) => s.clone(), Value::Int(n) => n.to_string() }
    }
}

#[derive(Clone, Debug)]
pub struct Col { pub name: String, pub ty: Type }

#[derive(Clone, Debug)]
pub struct RelDecl { pub name: String, pub cols: Vec<Col> }

#[derive(Clone, Debug)]
pub struct RelMeta { pub cols: Vec<Col> }

impl RelMeta {
    pub fn col_name(&self, i: usize) -> &str { &self.cols[i].name }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp { Eq, Ne, Lt, Le, Gt, Ge }

impl CmpOp {
    pub fn sql(self) -> &'static str {
        match self {
            CmpOp::Eq => "=", CmpOp::Ne => "<>",
            CmpOp::Lt => "<", CmpOp::Le => "<=",
            CmpOp::Gt => ">", CmpOp::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Term { Var(String), Str(String), Int(i64), Wild }

#[derive(Clone, Debug)]
pub struct Atom { pub rel: String, pub terms: Vec<Term> }

#[derive(Clone, Debug)]
pub struct Constraint { pub lhs: Term, pub op: CmpOp, pub rhs: Term }

#[derive(Clone, Debug)]
pub enum BodyItem {
    Pos(Atom),
    Neg(Atom),
    Scan { rev: Term, glob: Term, path: Term, rev_out: Term },
    Match { path: Term, rev: Term, regex: String, line: Term },
    Ast { path: Term, rev: Term, lang: String, query: String, line: Term, end: Option<Term> },
    Sg { path: Term, rev: Term, lang: String, pattern: String, line: Term },
    Json { path: Term, rev: Term, jpath: String, out: Term },
    Cmp(Constraint),
    /// Transitive closure of an edge relation, e.g. `reaches(a,b) <- closure(calls).`
    Closure { rel: String },
}

#[derive(Clone, Debug)]
pub struct Rule { pub head: Atom, pub body: Vec<BodyItem> }

impl Rule {
    pub fn is_source(&self) -> bool {
        self.body.iter().any(|b| matches!(b,
            BodyItem::Scan { .. } | BodyItem::Match { .. } | BodyItem::Ast { .. }
            | BodyItem::Sg { .. } | BodyItem::Json { .. }))
    }

    /// Some(edge) iff this rule is exactly `head(..) <- closure(edge).`
    pub fn closure_edge(&self) -> Option<&str> {
        match self.body.as_slice() {
            [BodyItem::Closure { rel }] => Some(rel),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Query { pub head: Atom, pub wheres: Vec<Constraint> }

#[derive(Clone, Debug)]
pub enum Item { Rel(RelDecl), Rule(Rule), Query(Query) }

#[derive(Clone, Debug, Default)]
pub struct Program { pub items: Vec<Item> }

pub type Rels = HashMap<String, RelMeta>;
