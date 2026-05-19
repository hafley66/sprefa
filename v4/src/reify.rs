//! `reify` — the TS→sprf macro op.
//!
//! Split of concerns:
//!   - `sprefa-extract` crate: source → `TyEntity` IR (knows langs).
//!   - here (stone 2): `emit_sprf` — `TyEntity` → sprf decl text.
//!   - here (stone 3): `expand_in_place` — a pre-pass that runs BEFORE
//!     `host_parse`: it finds each `reify(...)` statement, extracts,
//!     emits, and rewrites a comment-marked managed region right after
//!     the statement, then writes the file back. The unmodified
//!     parse→walk→run pipeline then sees the generated `rule(...)` as
//!     ordinary sprf (a true macro: expand + run).
//!   - `ReifyDef` (op): the `reify(...)` statement itself lowers to a
//!     no-op pipe; all its work already happened in the pre-pass.
//!
//! Region markers (idempotency by structural hash of the body):
//!   `#<reify gen h=HASH>` … `#</reify gen>`
//! Same source ⇒ same body ⇒ same hash ⇒ byte-identical region ⇒ the
//! pre-pass leaves the file untouched (no rewrite).

use std::hash::{Hash, Hasher};
use std::path::Path;

use effect_runtime::v2::Pipe;
use sprefa_extract::{LangExtract, RustExtract, TyEntity, TyRef};

use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{ArgKind, ArgSig, DslBody, DslShape, OperatorDef};
use crate::compile::lower::value::Value;
use crate::Cursor;

// ─── stone 2: TyEntity → sprf text ────────────────────────────────────────

/// `struct Point { x: i64, y: i64 }` ⇒
/// `rule(:Point, x?, y?);  # reify types: x: t.i64, y: t.i64`
///
/// v0 emits a STRUCTURAL decl only (plain `col?` columns) plus the
/// extracted types as a trailing `#` comment. Reason: sprf rule-decl
/// has no `col?: type` grammar yet — `x?: t.i64` misparses as a kwarg.
/// Typed columns ("a type is a value-space pipe; `t.*` resolves at
/// lower") are a SEPARATE initiative (project_types_in_value_space);
/// reify must not block on it. The type IR is retained in the comment
/// (visible, diffable, lossless) so the typed form can replace the
/// comment once value-space types land. Fieldless ⇒ `rule(:Name);`.
pub fn emit_sprf(e: &TyEntity) -> String {
    if e.fields.is_empty() {
        return format!("rule(:{});", e.name);
    }
    let cols = e
        .fields
        .iter()
        .map(|f| format!("{}?", f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let types = e
        .fields
        .iter()
        .map(|f| {
            let ty = match &f.ty {
                TyRef::Prim(p) => p.as_str(),
                TyRef::Named(n) => n.as_str(),
            };
            format!("{}: t.{}", f.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("rule(:{}, {});  # reify types: {}", e.name, cols, types)
}

fn body_hash(body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ─── stone 3: the pre-pass ─────────────────────────────────────────────────

const BEGIN: &str = "#<reify gen h=";
const END: &str = "#</reify gen>";

fn collect_rs_files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if matches!(name, "target" | ".git" | "node_modules") {
                continue;
            }
            collect_rs_files(&p, out);
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// Build the generated body (one decl per discovered struct, sorted by
/// name for determinism).
fn generate_body(root: &Path) -> String {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files);
    files.sort();
    let mut ents: Vec<TyEntity> = Vec::new();
    for f in &files {
        if let Ok(src) = std::fs::read_to_string(f) {
            ents.extend(RustExtract.extract_types(&src));
        }
    }
    ents.sort_by(|a, b| a.name.cmp(&b.name));
    ents.dedup();
    ents.iter().map(emit_sprf).collect::<Vec<_>>().join("\n")
}

/// Pre-pass. Returns `Ok(Some(new_src))` if the file was rewritten,
/// `Ok(None)` if there was nothing to do OR the region was already
/// byte-current (idempotent: no write, no churn).
pub fn expand_in_place(
    path: &Path,
    src: &str,
    root: &Path,
) -> std::io::Result<Option<String>> {
    // Locate the first `reify(` statement and its terminating `;`.
    let Some(rstart) = src.find("reify(") else {
        return Ok(None);
    };
    let Some(semi_rel) = src[rstart..].find(';') else {
        return Ok(None);
    };
    let semi = rstart + semi_rel;
    // Region goes on the line AFTER the statement.
    let after_line = match src[semi..].find('\n') {
        Some(nl) => semi + nl + 1,
        None => src.len(),
    };

    let body = generate_body(root);
    let hash = body_hash(&body);
    let region = format!("{BEGIN}{hash}>\n{body}\n{END}\n");

    // Existing managed region immediately after the statement?
    let tail = &src[after_line..];
    if let Some(b_rel) = tail.find(BEGIN) {
        // Only treat as the managed region if it is adjacent (only
        // whitespace between the statement line and the marker).
        if tail[..b_rel].trim().is_empty() {
            let b = after_line + b_rel;
            if let Some(e_rel) = src[b..].find(END) {
                let region_end = {
                    let e = b + e_rel + END.len();
                    match src[e..].find('\n') {
                        Some(nl) => e + nl + 1,
                        None => e,
                    }
                };
                let existing = &src[b..region_end];
                if existing == region {
                    return Ok(None); // byte-current ⇒ no write
                }
                let mut new_src = String::with_capacity(src.len());
                new_src.push_str(&src[..b]);
                new_src.push_str(&region);
                new_src.push_str(&src[region_end..]);
                std::fs::write(path, &new_src)?;
                return Ok(Some(new_src));
            }
        }
    }

    // No region yet: insert one after the statement line.
    let mut new_src = String::with_capacity(src.len() + region.len());
    new_src.push_str(&src[..after_line]);
    new_src.push_str(&region);
    new_src.push_str(&src[after_line..]);
    std::fs::write(path, &new_src)?;
    Ok(Some(new_src))
}

// ─── the op: inert at lower (work done by the pre-pass) ────────────────────

const REIFY_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "lang",
    doc: "language atom (:rs)",
    required: false,
}];

pub struct ReifyDef;

impl OperatorDef for ReifyDef {
    fn name(&self) -> &'static str {
        "reify"
    }
    fn paren_args(&self) -> &[ArgSig] {
        REIFY_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // The macro already expanded in `expand_in_place` before parse.
        // The statement itself contributes nothing at runtime.
        Ok(Pipe::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_extract::TyField;

    #[test]
    fn emit_struct_decl() {
        let e = TyEntity {
            name: "Point".into(),
            kind: "struct".into(),
            fields: vec![
                TyField {
                    name: "x".into(),
                    ty: TyRef::Prim("i64".into()),
                },
                TyField {
                    name: "y".into(),
                    ty: TyRef::Prim("i64".into()),
                },
            ],
        };
        assert_eq!(
            emit_sprf(&e),
            "rule(:Point, x?, y?);  # reify types: x: t.i64, y: t.i64"
        );
    }

    #[test]
    fn emit_named_and_fieldless() {
        let e = TyEntity {
            name: "Wrap".into(),
            kind: "struct".into(),
            fields: vec![TyField {
                name: "inner".into(),
                ty: TyRef::Named("Point".into()),
            }],
        };
        assert_eq!(
            emit_sprf(&e),
            "rule(:Wrap, inner?);  # reify types: inner: t.Point"
        );
        let f = TyEntity {
            name: "Unit".into(),
            kind: "struct".into(),
            fields: vec![],
        };
        assert_eq!(emit_sprf(&f), "rule(:Unit);");
    }
}
