//! `(doc.layout ?Doc ?Width ?Text)`: render a Doc sum term through `pretty`.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::kernel::kernel_ref;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IExecutor};
use pretty::RcDoc;
use std::time::Duration;

pub const RELATION: &str = "doc.layout";
const MAX_DOC_DEPTH: usize = 256;

pub struct DocLayout {
    relation: TermId,
}

impl DocLayout {
    pub fn new(relation: TermId) -> DocLayout {
        DocLayout { relation }
    }

    fn render(&self, u: &mut Universe, rows: &Store, term: TermId, width: usize) -> Option<String> {
        let doc = doc_term(u, rows, term, 0)?;
        let mut bytes = Vec::new();
        doc.render(width, &mut bytes).ok()?;
        String::from_utf8(bytes).ok()
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
            let Some(text) = self.render(u, store, *doc, width) else {
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

fn doc_term(
    u: &mut Universe,
    rows: &Store,
    term: TermId,
    depth: usize,
) -> Option<RcDoc<'static, ()>> {
    if depth >= MAX_DOC_DEPTH {
        tracing::warn!(target: "dl8::doc_layout", "document depth limit exceeded");
        return None;
    }
    let child_depth = depth + 1;
    let term = u.unary(term, "ref").unwrap_or(term);
    let [constructor, arguments] = u.args::<2>(term, "application")?;
    let variant = variant_name(u, rows, constructor)?;
    let arguments = u.as_list(arguments)?;
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
            let parts = u.as_list(*parts)?;
            Some(RcDoc::concat(
                parts
                    .into_iter()
                    .map(|part| doc_term(u, rows, part, child_depth))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        "nest" => {
            let [indent, body] = arguments.as_slice() else {
                return None;
            };
            let indent = isize::try_from(u.as_int(*indent)?).ok()?;
            Some(doc_term(u, rows, *body, child_depth)?.nest(indent))
        }
        "group" => {
            let [body] = arguments.as_slice() else {
                return None;
            };
            Some(doc_term(u, rows, *body, child_depth)?.group())
        }
        "line" if arguments.is_empty() => Some(RcDoc::line()),
        _ => None,
    }
}

fn variant_name(u: &mut Universe, rows: &Store, constructor: TermId) -> Option<String> {
    let colon = kernel_ref(u, ":");
    let table = rows.table(colon)?;
    let target = u.compound("ref", vec![constructor]);
    (0..table.len()).find_map(|index| {
        let row = table.row(index as u32);
        (row.get(2) == Some(&target))
            .then(|| {
                row.get(1)
                    .and_then(|label| term_name(u, *label).map(str::to_owned))
            })
            .flatten()
    })
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
