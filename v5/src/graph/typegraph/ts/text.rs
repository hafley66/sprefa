//! TS/TSX comment/template/unresolved-ref extraction (oxc front-end).
//! Pure code motion out of the former single typegraph.rs; zero behavior
//! change.

use oxc_ast::ast as ts_ast;
use oxc_ast_visit::Visit as OxcVisit;

use super::super::*;

/// Every comment in a TS/TSX file, grammar-backed by oxc's comment table
/// (`program.comments`). TS/TSX is NOT in the tree-sitter `AST_LANG_TABLE`
/// (oxc is the front-end), so the generic `cst::walk_comments` can't see it —
/// this is the TS arm of `comment_node`. oxc's `Comment.span` covers the FULL
/// comment INCLUDING delimiters (`//`, `/* */`), which is exactly the raw span
/// `comment_node` records; a `//` inside a string is a token, never a comment
/// row, because the lexer produced these (string-literal safety, the whole
/// point). Byte offsets are mapped to 1-based line / 0-based column via a line
/// index, matching the tree-sitter arm and the `sg`/`diag` convention.
pub fn ts_comments(content: &str, tsx: bool) -> Vec<crate::cst::RawComment> {
    let alloc = oxc_allocator::Allocator::default();
    let st = if tsx {
        oxc_span::SourceType::tsx()
    } else {
        oxc_span::SourceType::ts()
    };
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    // oxc still populates the comment table on a partial parse; `panicked` only
    // means the AST is incomplete, so comments are usable regardless.
    let idx = line_index(content);
    ret.program
        .comments
        .iter()
        .filter_map(|c| {
            let (lo, hi) = (c.span.start as usize, c.span.end as usize);
            let raw = content.get(lo..hi)?.to_string();
            let (sl, sc) = line_col(&idx, lo);
            let (el, ec) = line_col(&idx, hi);
            Some(crate::cst::RawComment {
                start_row: sl,
                start_col: sc,
                end_row: el,
                end_col: ec,
                raw,
            })
        })
        .collect()
}

/// One piece of a template literal, in source order: `` `GET /users/${id}` ``
/// splits into `[(static, "GET /users/"), (expr, "id")]`. `node` is the
/// `df_node`/`df_lit` id the DATAFLOW lift mints for the SAME occurrence —
/// `{file}:{anchor}:template` (`ts_push`'s exact `{file}:{byte_off}:{kind}`
/// scheme, `typegraph.rs`'s `fn ts_push`) — so a consumer joins a piece
/// straight to `df_lit`/`df_node`/`df_edge` with no extra id math: a
/// template's static chunk row (`kind = "static"`) joins `df_lit.id` (the
/// same template's raw-source `df_lit` row), and `node` joins `df_edge`'s
/// `to` column for whatever flows INTO the template (an interpolated var's
/// `var_read` node has its own edge `to = node`). `anchor` is the plain
/// template literal's own span start (the opening backtick); for a TAGGED
/// template it is the `TaggedTemplateExpression`'s own span start (the tag's
/// position, NOT the quasi's) — `ts_flow_expr`'s `off = span_off(e)` mints
/// the df id off the OUTER expression node for a tagged template, so
/// `template_parts` anchors there too rather than at the quasi (the two
/// walks would otherwise disagree on `node` for every tagged template in the
/// corpus). Shared by every piece of the SAME occurrence so a consumer
/// groups pieces by `node` and orders them by `idx`; stable across ticks for
/// unchanged content since it is derived from the byte content itself, not a
/// counter. `line` is the template literal's own 1-based start line (the
/// `comment_node`/`sg`/`diag` convention: 1-based line, byte offsets for
/// everything finer-grained).
///
/// A nested template literal (an interpolation whose value is itself a
/// template, e.g. `` `outer ${`inner ${x}`}` ``) mints its OWN independent
/// node/idx sequence — the outer occurrence's `expr` piece for that slot
/// still carries the nested template's full verbatim source text (backticks
/// included), the same treatment any other expression gets.
#[derive(Clone, Debug)]
pub struct TemplatePart {
    pub node: String,
    pub line: u32,
    /// 0-based byte column of the occurrence anchor — the `col` component of the
    /// `node` coordinate, retained so `template_parts.node` can be resolved to
    /// the SAME `_df_node_dict` surrogate as `df_lit.id` (identity
    /// normalization, 2026-07-20).
    pub col: u32,
    pub idx: u32,
    pub kind: &'static str,
    pub text: String,
}

/// Every template-literal piece in a TS/TSX/JS/JSX file (`template_parts`'
/// TS-family extractor; the walk needs the byte offsets `line_index` and a
/// content slice, not `program.comments`, so it takes `file`/`content`
/// directly rather than sharing `ts_comments`' `tsx: bool` shape). Dispatch by
/// extension via `source_type_for`, matching `TsTypes::extract`'s file set
/// (`.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`).
pub fn ts_template_parts(file: &str, content: &str) -> Vec<TemplatePart> {
    let alloc = oxc_allocator::Allocator::default();
    let st = source_type_for(file);
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    let starts = line_index(content);
    let mut walker = TsTemplateWalker {
        file,
        content,
        starts: &starts,
        out: Vec::new(),
        tag_anchor: None,
    };
    walker.visit_program(&ret.program);
    walker.out
}

struct TsTemplateWalker<'s> {
    file: &'s str,
    content: &'s str,
    starts: &'s [usize],
    out: Vec<TemplatePart>,
    /// Set by `visit_tagged_template_expression` right before the walk
    /// descends into `it.quasi` (oxc dispatches a tagged template's quasi
    /// through `visit_template_literal`, same as a plain template — see the
    /// doc comment on `TemplatePart`); consumed (taken) by the very next
    /// `visit_template_literal` call, which is exactly that quasi. Any
    /// FURTHER nested template reached during that same walk (an
    /// interpolation whose value is itself a template) sees `None` again by
    /// then and anchors at its own span start, unaffected.
    tag_anchor: Option<u32>,
}

impl<'a, 's> OxcVisit<'a> for TsTemplateWalker<'s> {
    fn visit_tagged_template_expression(&mut self, it: &ts_ast::TaggedTemplateExpression<'a>) {
        // Matches `ts_flow_expr`'s `off = span_off(e)` for the WHOLE
        // `TaggedTemplateExpression` — the tag's own span start, not the
        // quasi's — so `df_lit`'s id for this occurrence and this walk's
        // `node` agree exactly.
        let prev = self.tag_anchor.replace(it.span.start);
        oxc_ast_visit::walk::walk_tagged_template_expression(self, it);
        self.tag_anchor = prev;
    }

    fn visit_template_literal(&mut self, it: &ts_ast::TemplateLiteral<'a>) {
        let anchor = self.tag_anchor.take().unwrap_or(it.span.start);
        // Mirror `ts_push`'s id scheme EXACTLY (`{file}:{line}:{col}:{kind}`,
        // 1-based line + 0-based byte col via `line_col`) so this `node` id joins
        // `df_lit.id`/`df_node.id`/`df_edge.to` for the same occurrence. The df
        // coordinate de-intern reconstructs these ids from (file,line,col,kind),
        // so the byte-offset scheme this used before no longer matches.
        let (line, col) = line_col(self.starts, anchor as usize);
        let node = format!("{}:{line}:{col}:template", self.file);
        let mut idx = 0u32;
        // `quasis`/`expressions` strictly alternate (quasis.len() ==
        // expressions.len() + 1): static, expr, static, expr, ..., static. An
        // empty static chunk (adjacent interpolations, `` `${a}${b}` ``) still
        // emits its own row with `text = ""` — never skipped, so `idx` always
        // matches the piece's real position and an empty template (a bare
        // `` ` ` ``) still yields one static row.
        for (slot, quasi) in it.quasis.iter().enumerate() {
            self.out.push(TemplatePart {
                node: node.clone(),
                line,
                col,
                idx,
                kind: "static",
                text: quasi.value.raw.to_string(),
            });
            idx += 1;
            if let Some(expr) = it.expressions.get(slot) {
                use oxc_span::GetSpan;
                let span = expr.span();
                let text = self
                    .content
                    .get(span.start as usize..span.end as usize)
                    .unwrap_or_default()
                    .to_string();
                self.out.push(TemplatePart {
                    node: node.clone(),
                    line,
                    col,
                    idx,
                    kind: "expr",
                    text,
                });
                idx += 1;
            }
        }
        // Recurse: a tagged template's own tag expression, and any nested
        // template literal inside an interpolation, get their own
        // `visit_template_literal` call through the normal walk (oxc dispatches
        // `Expression::TemplateLiteral` and `TaggedTemplateExpression.quasi`
        // both through this method), minting their own independent node/idx
        // sequence rather than being folded into this one.
        oxc_ast_visit::walk::walk_template_literal(self, it);
    }
}

/// One `unresolved` marker occurrence: an edge that COULD exist but whose
/// target is computed at runtime rather than a static literal — as opposed to
/// `module_unresolved`, which flags a specifier that resolved to NO project
/// file at all (a genuinely missing target, a different flavor this rel does
/// NOT duplicate). `unresolved`'s own TS/JS-only oxc walk (own
/// `ExtractFamily`, no cross-family reads, so its digest stays self-contained,
/// matching the `template_parts`/`comment_node` precedent) covers three reason
/// buckets, each re-derived from an AST shape another pass in this file
/// already visits for a different purpose, never a wholly new detection
/// concept:
///
/// - `dynamic-import`: a `import(expr)` / `require(expr)` call whose argument
///   is not a plain string literal. The ES grammar requires a static `import
///   ... from` specifier to be a literal, so a computed specifier can only
///   ever show up in call form — the same "not a literal" signal
///   `module_unresolved`'s `"{spec}: dynamic"` case already flags for the
///   template-literal-interpolated case the modgraph regex resolver sees.
/// - `computed-member-call`: `obj[key]()` — the call-site walk that resolves
///   `a.b.c()` to `"c"` (`ts_callee_name`) already visits this exact callee
///   shape and silently drops it today.
/// - `spread-call-args`: `f(...args)` — the dataflow arg walk (`ts_flow_call`)
///   already iterates `c.arguments` and silently drops a `SpreadElement` via
///   `arg.as_expression()` returning `None`.
///
/// `detail` is the computed thing's exact source text, verbatim. `line` is
/// 1-based (the `comment_node`/`sg`/`diag` convention).
///
/// OUT of v1 scope, on purpose: Python star-imports and `sys.path` mutation
/// (both already surfaced today — `module_unresolved`'s `"star import not
/// expanded"` row and a loud `eprintln`, respectively) are not unioned in
/// here, to avoid a cross-family digest dependency (this family's digest
/// would otherwise need to key off `module_unresolved`'s content, not just
/// its own TS/JS file set — the exact "hidden cross-family dependency" shape
/// flagged as a debt item elsewhere). A future widening can revisit this once
/// a safe cross-family digest composition exists.
#[derive(Clone, Debug)]
pub struct UnresolvedRef {
    pub line: u32,
    pub reason: &'static str,
    pub detail: String,
}

/// Every `unresolved` marker in a TS/TSX/JS/JSX/MJS/CJS file (see
/// `UnresolvedRef`).
pub fn ts_unresolved_refs(file: &str, content: &str) -> Vec<UnresolvedRef> {
    let alloc = oxc_allocator::Allocator::default();
    let st = source_type_for(file);
    let ret = oxc_parser::Parser::new(&alloc, content, st).parse();
    if ret.panicked {
        return Vec::new();
    }
    let starts = line_index(content);
    let mut walker = TsUnresolvedWalker {
        content,
        starts: &starts,
        out: Vec::new(),
    };
    walker.visit_program(&ret.program);
    walker.out
}

struct TsUnresolvedWalker<'s> {
    content: &'s str,
    starts: &'s [usize],
    out: Vec<UnresolvedRef>,
}

impl<'s> TsUnresolvedWalker<'s> {
    fn slice(&self, span: oxc_span::Span) -> String {
        self.content
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
            .to_string()
    }
}

impl<'a, 's> OxcVisit<'a> for TsUnresolvedWalker<'s> {
    fn visit_import_expression(&mut self, it: &ts_ast::ImportExpression<'a>) {
        if !matches!(it.source, ts_ast::Expression::StringLiteral(_)) {
            use oxc_span::GetSpan;
            self.out.push(UnresolvedRef {
                line: line_at(self.starts, it.span.start as usize),
                reason: "dynamic-import",
                detail: self.slice(it.source.span()),
            });
        }
        oxc_ast_visit::walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &ts_ast::CallExpression<'a>) {
        use oxc_span::GetSpan;
        // `require(expr)`: only a bare `require` callee counts (matching the
        // module resolver's own CJS convention), and only when the sole
        // argument isn't a plain string literal — a static string keeps the
        // dependency statically resolvable, already handled by
        // `module_import`/`module_unresolved`.
        if let ts_ast::Expression::Identifier(callee) = &it.callee {
            if callee.name == "require" {
                if let Some(arg) = it.arguments.first().and_then(|a| a.as_expression()) {
                    if !matches!(arg, ts_ast::Expression::StringLiteral(_)) {
                        self.out.push(UnresolvedRef {
                            line: line_at(self.starts, it.span.start as usize),
                            reason: "dynamic-import",
                            detail: self.slice(arg.span()),
                        });
                    }
                }
            }
        }
        // `obj[key]()`: a computed-member callee, the shape `ts_callee_name`
        // already recognizes and silently drops (returns `None`).
        if let ts_ast::Expression::ComputedMemberExpression(m) = &it.callee {
            self.out.push(UnresolvedRef {
                line: line_at(self.starts, m.span.start as usize),
                reason: "computed-member-call",
                detail: self.slice(m.span),
            });
        }
        // `f(...args)`: a spread argument, the shape `ts_flow_call`'s arg loop
        // already visits and silently drops (`arg.as_expression()` is `None`
        // for `Argument::SpreadElement`).
        for arg in &it.arguments {
            if let ts_ast::Argument::SpreadElement(sp) = arg {
                self.out.push(UnresolvedRef {
                    line: line_at(self.starts, sp.span.start as usize),
                    reason: "spread-call-args",
                    detail: self.slice(sp.span),
                });
            }
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parts_static_then_expr_then_static() {
        let src = "const route = `GET /users/${userId}/posts`;\n";
        let parts = ts_template_parts("route.ts", src);
        assert_eq!(parts.len(), 3, "{:?}", parts);
        assert_eq!(
            (parts[0].idx, parts[0].kind, parts[0].text.as_str()),
            (0, "static", "GET /users/")
        );
        assert_eq!(
            (parts[1].idx, parts[1].kind, parts[1].text.as_str()),
            (1, "expr", "userId")
        );
        assert_eq!(
            (parts[2].idx, parts[2].kind, parts[2].text.as_str()),
            (2, "static", "/posts")
        );
        // one occurrence: every piece shares the same node id.
        assert_eq!(parts[0].node, parts[1].node);
        assert_eq!(parts[1].node, parts[2].node);
    }

    #[test]
    fn template_parts_adjacent_statics_and_expr_only() {
        // `${a}${b}`: quasis/expressions strictly alternate (quasis.len() ==
        // expressions.len() + 1), so back-to-back interpolations with no
        // literal text between them still produce an (empty) static row
        // between them — idx never skips a slot.
        let src = "const both = `${a}${b}`;\nconst justExpr = `${onlyExpr}`;\n";
        let both = ts_template_parts("both.ts", src);
        let first_node = both[0].node.clone();
        let first_occurrence: Vec<_> = both.iter().filter(|p| p.node == first_node).collect();
        assert_eq!(first_occurrence.len(), 5, "{:?}", both);
        assert_eq!(
            first_occurrence
                .iter()
                .map(|p| (p.idx, p.kind, p.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "static", ""),
                (1, "expr", "a"),
                (2, "static", ""),
                (3, "expr", "b"),
                (4, "static", "")
            ],
        );
        // second template: expr-only occurrence still opens and closes with
        // (empty) static chunks around the single interpolation.
        let second_node = both
            .iter()
            .map(|p| p.node.clone())
            .find(|n| *n != first_node)
            .expect("second node");
        let second_occurrence: Vec<_> = both.iter().filter(|p| p.node == second_node).collect();
        assert_eq!(
            second_occurrence
                .iter()
                .map(|p| (p.idx, p.kind, p.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "static", ""),
                (1, "expr", "onlyExpr"),
                (2, "static", "")
            ],
        );
    }

    #[test]
    fn template_parts_empty_template_yields_one_static_row() {
        let src = "const blank = ``;\n";
        let parts = ts_template_parts("blank.ts", src);
        assert_eq!(parts.len(), 1, "{:?}", parts);
        assert_eq!(
            (parts[0].idx, parts[0].kind, parts[0].text.as_str()),
            (0, "static", "")
        );
    }

    #[test]
    fn template_parts_backtick_escapes_stay_verbatim() {
        // raw (not cooked): \n stays as the two source characters backslash+n,
        // and an escaped backtick/dollar stays escaped, exactly as written.
        let src = r#"const s = `line one\nline two \` and \${notAnExpr}`;
"#;
        let parts = ts_template_parts("esc.ts", src);
        assert_eq!(parts.len(), 1, "{:?}", parts);
        assert_eq!(parts[0].kind, "static");
        assert_eq!(parts[0].text, r"line one\nline two \` and \${notAnExpr}");
    }

    #[test]
    fn template_parts_nested_template_mints_its_own_node() {
        // the outer's interpolation slot is itself a template literal; it gets
        // its own independent node/idx sequence, while the outer's `expr`
        // piece for that slot carries the nested template's full source text.
        let src = "const s = `outer ${`inner ${value}`}`;\n";
        let parts = ts_template_parts("nested.ts", src);
        let nodes: std::collections::HashSet<String> =
            parts.iter().map(|p| p.node.clone()).collect();
        assert_eq!(nodes.len(), 2, "{:?}", parts);

        let outer_node = parts
            .iter()
            .find(|p| p.text == "outer ")
            .expect("outer static")
            .node
            .clone();
        let outer: Vec<_> = parts.iter().filter(|p| p.node == outer_node).collect();
        // one interpolation slot -> 2 quasis (leading "outer ", trailing "")
        // plus the 1 expr piece in between.
        assert_eq!(
            outer
                .iter()
                .map(|p| (p.idx, p.kind, p.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "static", "outer "),
                (1, "expr", "`inner ${value}`"),
                (2, "static", "")
            ],
        );

        let inner_node = parts
            .iter()
            .find(|p| p.text == "value")
            .expect("inner expr")
            .node
            .clone();
        assert_ne!(inner_node, outer_node);
        let inner: Vec<_> = parts.iter().filter(|p| p.node == inner_node).collect();
        assert_eq!(
            inner
                .iter()
                .map(|p| (p.idx, p.kind, p.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "static", "inner "),
                (1, "expr", "value"),
                (2, "static", "")
            ],
        );
    }

    #[test]
    fn template_parts_tagged_template_uses_quasi_pieces() {
        // `` styled.div`color: ${c}` ``: the tag isn't part of the split, only
        // the quasi (the backtick-delimited literal) is.
        let src = "const box = styled.div`color: ${c};`;\n";
        let parts = ts_template_parts("tagged.ts", src);
        assert_eq!(
            parts
                .iter()
                .map(|p| (p.idx, p.kind, p.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0, "static", "color: "),
                (1, "expr", "c"),
                (2, "static", ";")
            ],
        );
    }

    // --- string-values arc (df_lit + const_value + concat) ---
}
