//! BindingGraph — static, lower-time analysis of term writers and readers.
//!
//! Walks a host CST `pipe` node, threads an environment of currently-bound
//! names, and produces:
//!   - reader-site → writer-site resolutions (def-use map)
//!   - diagnostics for unbound-Read and shadowed-Unbound term refs
//!   - the set of names guaranteed bound on every cursor exiting the pipe
//!
//! Scope (v3 minimum):
//!   - Single linear `pipe` (Seq). Brace-slot fork merging is a TODO;
//!     for now brace-slot bodies are not recursed.
//!   - Term refs surfaced from `paren_slot` children only:
//!       * direct `term_ref` (bare `$X` / `$X?`)
//!       * `carveout_expr` whose body is `term_ref` (`${X?}`)
//!       * `carveout_expr` whose body is a single bare-IDENT op_invocation
//!         (`${X}` Read shorthand)
//!   - Reserved synthesized names `REPO` / `REV` / `FS` are seeded into
//!     the root env. Source ops that bind those names overwrite the
//!     reserved entry with their own writer site.
//!   - Rule calls and parametric rules are out of scope; the walk treats
//!     every op_invocation uniformly.
//!
//! Out of scope (filed as separate work):
//!   - Fork arm-merge intersection (sprefa-9lt step 5)
//!   - Cross-rule export bindings (sprefa-4m7.5.x)
//!   - Carveout pipe-as-arg recursion (sprefa-4iv)

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use tree_sitter::Node;

use crate::value::TermMode;

/// Result of analyzing one pipe.
#[derive(Debug, Default)]
pub struct BindingReport {
    pub diagnostics: Vec<BindingDiagnostic>,
    pub resolutions: Vec<TermResolution>,
    pub exit_bindings: Vec<Arc<str>>,
}

#[derive(Debug, Clone)]
pub struct BindingDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub byte_range: Range<usize>,
}

/// One reader (`${X}` Read) → writer (`${X?}` step) link.
/// `writer_byte_range` is `None` for resolutions to reserved synthesized
/// names (`REPO`/`REV`/`FS`) that have no concrete writer site.
#[derive(Debug, Clone)]
pub struct TermResolution {
    pub name: Arc<str>,
    pub reader_byte_range: Range<usize>,
    pub writer_byte_range: Option<Range<usize>>,
}

/// Analyze a single `pipe` node (kind == "pipe").
pub fn analyze_pipe(pipe_node: Node<'_>, src: &[u8]) -> BindingReport {
    let mut env: HashMap<Arc<str>, Option<Range<usize>>> = HashMap::new();
    for reserved in ["REPO", "REV", "FS"] {
        env.insert(Arc::from(reserved), None);
    }

    let mut report = BindingReport::default();

    let mut walk = pipe_node.walk();
    for step in pipe_node.named_children(&mut walk) {
        if step.kind() != "op_invocation" {
            continue;
        }
        let Some(paren) = step.child_by_field_name("paren") else {
            continue;
        };
        analyze_paren_slot(paren, src, &mut env, &mut report);
    }

    report.exit_bindings = env.keys().cloned().collect();
    report.exit_bindings.sort();
    report
}

fn analyze_paren_slot(
    paren: Node<'_>,
    src: &[u8],
    env: &mut HashMap<Arc<str>, Option<Range<usize>>>,
    report: &mut BindingReport,
) {
    let mut walk = paren.walk();
    for child in paren.named_children(&mut walk) {
        match child.kind() {
            "term_ref" => {
                let (name, mode) = parse_term_ref(child, src);
                handle_term(name, mode, child.byte_range(), env, report);
            }
            "carveout_expr" => {
                if let Some(body) = child.child_by_field_name("body") {
                    match body.kind() {
                        "term_ref" => {
                            let (name, mode) = parse_term_ref(body, src);
                            handle_term(name, mode, body.byte_range(), env, report);
                        }
                        "pipe" => {
                            if let Some((name, range)) = single_bare_ident_pipe(body, src) {
                                handle_term(name, TermMode::Read, range, env, report);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn handle_term(
    name: Arc<str>,
    mode: TermMode,
    site: Range<usize>,
    env: &mut HashMap<Arc<str>, Option<Range<usize>>>,
    report: &mut BindingReport,
) {
    if name.is_empty() {
        return;
    }
    match mode {
        TermMode::Read => match env.get(&name) {
            Some(writer) => {
                report.resolutions.push(TermResolution {
                    name: name.clone(),
                    reader_byte_range: site,
                    writer_byte_range: writer.clone(),
                });
            }
            None => {
                report.diagnostics.push(BindingDiagnostic {
                    code: "term/unbound",
                    message: format!(
                        "${{{name}}} read but no upstream writer; \
                         introduce with ${{{name}?}} earlier in the pipe"
                    ),
                    byte_range: site,
                });
            }
        },
        TermMode::Unbound => {
            if let Some(prev) = env.get(&name) {
                let prev_msg = match prev {
                    Some(_) => "shadows an earlier ${X?} writer",
                    None => "shadows a reserved synthesized name (REPO/REV/FS)",
                };
                report.diagnostics.push(BindingDiagnostic {
                    code: "term/shadowed",
                    message: format!("${{{name}?}} {prev_msg}"),
                    byte_range: site.clone(),
                });
            }
            env.insert(name, Some(site));
        }
    }
}

fn parse_term_ref(node: Node<'_>, src: &[u8]) -> (Arc<str>, TermMode) {
    let bytes = &src[node.byte_range()];
    let rest = if bytes.first() == Some(&b'$') { &bytes[1..] } else { bytes };
    let rest = if rest.first() == Some(&b'{') && rest.last() == Some(&b'}') {
        &rest[1..rest.len() - 1]
    } else {
        rest
    };
    let (name_bytes, mode) = if rest.last() == Some(&b'?') {
        (&rest[..rest.len() - 1], TermMode::Unbound)
    } else {
        (rest, TermMode::Read)
    };
    (Arc::from(std::str::from_utf8(name_bytes).unwrap_or("")), mode)
}

/// `${X}` lowers to a pipe of one bare-IDENT op_invocation. Detect that
/// shape and return (name, byte_range_of_ident).
fn single_bare_ident_pipe(pipe: Node<'_>, src: &[u8]) -> Option<(Arc<str>, Range<usize>)> {
    let mut walk = pipe.walk();
    let invs: Vec<Node<'_>> = pipe.named_children(&mut walk)
        .filter(|n| n.kind() == "op_invocation")
        .collect();
    if invs.len() != 1 {
        return None;
    }
    let inv = invs[0];
    if inv.child_by_field_name("paren").is_some()
        || inv.child_by_field_name("brace").is_some()
        || inv.child_by_field_name("bracket").is_some()
    {
        return None;
    }
    let name_node = inv.child_by_field_name("name")?;
    let text = std::str::from_utf8(&src[name_node.byte_range()]).ok()?;
    Some((Arc::from(text), name_node.byte_range()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn pipe_of(src: &str) -> (Arc<tree_sitter::Tree>, Range<usize>) {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_sprefa::LANGUAGE.into()).unwrap();
        let tree = Arc::new(p.parse(src, None).unwrap());
        let pipe = tree.root_node().named_child(0).expect("at least one stmt");
        let r = pipe.byte_range();
        (tree, r)
    }

    fn analyze(src: &str) -> BindingReport {
        let (tree, _) = pipe_of(src);
        let pipe = tree.root_node().named_child(0).unwrap();
        analyze_pipe(pipe, src.as_bytes())
    }

    #[test]
    fn unbound_introducer_then_read_resolves() {
        let r = analyze("repo(${R?}) > rev(${R})");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.resolutions.len(), 1);
        assert_eq!(&*r.resolutions[0].name, "R");
        assert!(r.resolutions[0].writer_byte_range.is_some());
    }

    #[test]
    fn read_without_writer_diagnoses() {
        let r = analyze("rev(${X})");
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code, "term/unbound");
        assert!(r.resolutions.is_empty());
    }

    #[test]
    fn shadow_diagnoses_but_continues() {
        let r = analyze("repo(${R?}) > rev(${R?})");
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code, "term/shadowed");
    }

    #[test]
    fn reserved_repo_rev_fs_are_pre_bound() {
        let r = analyze("rev(${REPO})");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.resolutions.len(), 1);
        assert!(r.resolutions[0].writer_byte_range.is_none(),
            "reserved names have no concrete writer site");
    }

    #[test]
    fn bare_term_ref_introducer_works() {
        let r = analyze("repo($R?) > rev($R)");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.resolutions.len(), 1);
    }

    #[test]
    fn exit_bindings_include_writers_and_reserved() {
        let r = analyze("repo(${R?}) > rev(${V?})");
        let names: Vec<&str> = r.exit_bindings.iter().map(|a| a.as_ref()).collect();
        assert!(names.contains(&"R"));
        assert!(names.contains(&"V"));
        assert!(names.contains(&"REPO"));
        assert!(names.contains(&"REV"));
        assert!(names.contains(&"FS"));
    }
}
