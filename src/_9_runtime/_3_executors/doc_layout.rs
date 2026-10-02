//! `(doc.layout ?Doc ?Width ?Text)`: render a Doc sum term through `pretty`.
//! The `fs.file` sink renders its documents with the same `render`.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::kernel::kernel_ref;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IExecutor};
use pretty::RcDoc;
use std::collections::HashMap;
use std::time::Duration;

pub const RELATION: &str = "doc.layout";
const MAX_DOC_DEPTH: usize = 256;
/// Items one `concat` chain may unroll: the left spine a fold of `doc.join`
/// builds, one level per item.
const MAX_CONCAT_SPINE: usize = 1 << 20;

pub struct DocLayout {
    relation: TermId,
}

impl DocLayout {
    pub fn new(relation: TermId) -> DocLayout {
        DocLayout { relation }
    }
}

impl IExecutor for DocLayout {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn answer(&mut self, u: &mut Universe, store: &Store, pending: &[TermId]) -> Vec<Row> {
        let mut rows = Vec::with_capacity(pending.len());
        let variants = store_variants(u, store);
        for &application in pending {
            let Some(values) = application_values(u, application) else {
                continue;
            };
            let (Some(Some(doc)), Some(Some(width_term))) = (values.first(), values.get(1)) else {
                continue;
            };
            let Some(width) = u
                .as_int(*width_term)
                .and_then(|width| usize::try_from(width).ok())
            else {
                continue;
            };
            let Some(text) = render(u, &variants, *doc, width) else {
                continue;
            };
            let doc = u.compound("ref", vec![*doc]);
            let width = u.compound("const", vec![*width_term]);
            let text = u.string(&text);
            let text = u.compound("const", vec![text]);
            rows.push(Row {
                rel: self.relation,
                args: vec![doc, width, text],
            });
        }
        rows
    }

    fn poll(&mut self, _u: &mut Universe, _timeout: Duration) -> Vec<Row> {
        Vec::new()
    }

    fn armed(&self) -> bool {
        false
    }
}

/// Constructor to variant label, read off the `:` rows: `(: Owner Label
/// ref(Constructor) Index)`. The first row for a constructor names it.
pub type Variants = HashMap<TermId, String>;

/// The variants of the rows of one store.
pub fn store_variants(u: &mut Universe, rows: &Store) -> Variants {
    let colon = kernel_ref(u, ":");
    let mut variants = Variants::new();
    let Some(table) = rows.table(colon) else {
        return variants;
    };
    for index in 0..table.len() {
        let row = table.row(index as u32);
        if let (Some(label), Some(target)) = (row.get(1), row.get(2)) {
            insert_variant(u, &mut variants, *label, *target);
        }
    }
    variants
}

/// The variants of compiler rows `call(ref(kernel(:)), [Owner, Label, Target, Index])`.
pub fn row_variants(u: &mut Universe, compiler_rows: &[TermId]) -> Variants {
    let colon = kernel_ref(u, ":");
    let mut variants = Variants::new();
    for row in compiler_rows {
        let Some([rel, args]) = u.args::<2>(*row, "call") else {
            continue;
        };
        if rel != colon {
            continue;
        }
        let args = u.as_list(args).unwrap_or_default();
        if let [_, label, target, _] = args[..] {
            insert_variant(u, &mut variants, label, target);
        }
    }
    variants
}

fn insert_variant(u: &Universe, variants: &mut Variants, label: TermId, target: TermId) {
    let Some(constructor) = u.unary(target, "ref") else {
        return;
    };
    if variants.contains_key(&constructor) {
        return;
    }
    if let Some(name) = term_name(u, label) {
        variants.insert(constructor, name.to_owned());
    }
}

/// One Doc term laid out at `width`.
pub fn render(u: &mut Universe, variants: &Variants, term: TermId, width: usize) -> Option<String> {
    let doc = doc_term(u, variants, term, 0)?;
    let mut bytes = Vec::new();
    doc.render(width, &mut bytes).ok()?;
    String::from_utf8(bytes).ok()
}

/// `(variant, arguments)` of one Doc application, `ref`-wrapped or bare.
fn doc_parts(u: &mut Universe, variants: &Variants, term: TermId) -> Option<(String, Vec<TermId>)> {
    let term = u.unary(term, "ref").unwrap_or(term);
    let [constructor, arguments] = u.args::<2>(term, "application")?;
    let variant = variants.get(&constructor)?.clone();
    Some((variant, u.as_list(arguments)?))
}

/// The parts of a `concat` whose first part is a `concat`, and so on down the
/// left spine, as one list: a fold of `doc.join` nests one level per item.
fn concat_spine(u: &mut Universe, variants: &Variants, parts: TermId) -> Option<Vec<TermId>> {
    let mut tails: Vec<Vec<TermId>> = Vec::new();
    let mut head = tuple_parts(u, parts)?;
    for _ in 0..MAX_CONCAT_SPINE {
        let nested = match head.first() {
            Some(first) => match doc_parts(u, variants, *first) {
                Some((variant, arguments)) if variant == "concat" && arguments.len() == 1 => {
                    Some(tuple_parts(u, arguments[0])?)
                }
                _ => None,
            },
            None => None,
        };
        let Some(nested) = nested else {
            let mut out = head;
            for tail in tails.into_iter().rev() {
                out.extend(tail);
            }
            return Some(out);
        };
        tails.push(head[1..].to_vec());
        head = nested;
    }
    tracing::warn!(target: "dl8::doc_layout", "concat spine limit exceeded");
    None
}

fn doc_term(
    u: &mut Universe,
    variants: &Variants,
    term: TermId,
    depth: usize,
) -> Option<RcDoc<'static, ()>> {
    if depth >= MAX_DOC_DEPTH {
        tracing::warn!(target: "dl8::doc_layout", "document depth limit exceeded");
        return None;
    }
    let child_depth = depth + 1;
    let (variant, arguments) = doc_parts(u, variants, term)?;
    match variant.as_str() {
        "text" => {
            let [text] = arguments.as_slice() else {
                return None;
            };
            let Term::Str(value) = u.get(*text) else {
                return None;
            };
            Some(RcDoc::text(u.sym_str(*value).to_owned()))
        }
        "concat" => {
            let [parts] = arguments.as_slice() else {
                return None;
            };
            let parts = concat_spine(u, variants, *parts)?;
            Some(RcDoc::concat(
                parts
                    .into_iter()
                    .map(|part| doc_term(u, variants, part, child_depth))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        "nest" => {
            let [indent, body] = arguments.as_slice() else {
                return None;
            };
            let indent = isize::try_from(u.as_int(*indent)?).ok()?;
            Some(doc_term(u, variants, *body, child_depth)?.nest(indent))
        }
        "group" => {
            let [body] = arguments.as_slice() else {
                return None;
            };
            Some(doc_term(u, variants, *body, child_depth)?.group())
        }
        "line" if arguments.is_empty() => Some(RcDoc::line()),
        _ => None,
    }
}

/// `application(tuple, Items)`, `ref`-wrapped or bare: the positional
/// elements of one anonymous product.
fn tuple_parts(u: &mut Universe, term: TermId) -> Option<Vec<TermId>> {
    let term = u.unary(term, "ref").unwrap_or(term);
    let [constructor, items] = u.args::<2>(term, "application")?;
    let tuple = u.atom("tuple");
    (u.get(constructor) == u.get(tuple)).then_some(())?;
    u.as_list(items)
}

fn term_name(u: &Universe, mut term: TermId) -> Option<&str> {
    for _ in 0..2 {
        if let Some(inner) = u.unary(term, "ref").or_else(|| u.unary(term, "const")) {
            term = inner;
            continue;
        }
        return match u.get(term) {
            Term::Atom(name) => Some(u.sym_str(*name)),
            Term::Str(name) => Some(u.sym_str(*name)),
            _ => None,
        };
    }
    None
}
