//! Parse-tier guard for engine-owned relation names.
//!
//! The declaration catalog in `engine::decls` remains the single source of
//! truth. This small frontend-facing seam turns its reserved-name set into a
//! normal type diagnostic, so `--parse-only` catches the collision before a
//! scan/tick reaches `declare_all`.

use crate::ast::{Item, Severity, TypeDiag};

pub fn diags(prog: &crate::ast::Program, path: &str) -> Vec<TypeDiag> {
    let reserved = crate::engine::builtin_rel_names();
    prog.items.iter().filter_map(|item| {
        let Item::Rel(decl) = item else { return None; };
        reserved.contains(&decl.name).then(|| TypeDiag {
            path: path.to_string(),
            span: (0, 0),
            severity: Severity::Error,
            code: "reserved-name".into(),
            msg: format!(
                "relation `{}` is reserved by the engine; remove `rel {}(...)` and write to the built-in directly, or pick another name",
                decl.name, decl.name),
        })
    }).collect()
}
