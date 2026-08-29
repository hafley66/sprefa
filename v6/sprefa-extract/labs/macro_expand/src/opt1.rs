use ra_ap_mbe::DeclarativeMacro;
use ra_ap_parser::Edition;
use ra_ap_span::{Span, SpanAnchor, SyntaxContext, ROOT_ERASED_FILE_AST_ID};
use ra_ap_syntax::{
    ast, AstNode, SourceFile, SyntaxNode, TextRange,
    ast::HasName,
};
use ra_ap_syntax_bridge::{
    syntax_node_to_token_tree, token_tree_to_syntax_node, DocCommentDesugarMode, SpanMapper,
};
use ra_ap_tt::TopSubtree;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

struct FileSpanMap;

impl SpanMapper for FileSpanMap {
    fn span_for(&self, range: TextRange) -> Span {
        Span {
            range,
            anchor: SpanAnchor {
                file_id: ra_ap_span::EditionedFileId::new(
                    ra_ap_span::FileId::from_raw(0),
                    Edition::CURRENT,
                ),
                ast_id: ROOT_ERASED_FILE_AST_ID,
            },
            ctx: SyntaxContext::root(Edition::CURRENT),
        }
    }
}

#[salsa::db]
#[derive(Default)]
struct Db {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Db {}

fn edition(_ctx: SyntaxContext) -> Edition {
    Edition::CURRENT
}

fn collect_rules(
    node: &SyntaxNode,
    defs: &mut HashMap<String, (TopSubtree, TextRange)>,
) {
    for ev in node.preorder() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else { continue };
        if let Some(mr) = ast::MacroRules::cast(n.clone()) {
            let name = mr.name().map(|n| n.text().to_string());
            if let (Some(name), Some(tt)) = (name, mr.token_tree()) {
                let top =
                    syntax_node_to_token_tree(tt.syntax(), FileSpanMap, Span {
                        range: tt.syntax().text_range(),
                        anchor: SpanAnchor {
                            file_id: ra_ap_span::EditionedFileId::new(
                                ra_ap_span::FileId::from_raw(0),
                                Edition::CURRENT,
                            ),
                            ast_id: ROOT_ERASED_FILE_AST_ID,
                        },
                        ctx: SyntaxContext::root(Edition::CURRENT),
                    }, DocCommentDesugarMode::Mbe);
                defs.insert(name, (top, mr.syntax().text_range()));
            }
        }
        if let Some(md) = ast::MacroDef::cast(n.clone()) {
            // `macro m { .. }` (macro 2.0) definitions; rare, skip unless present
            let _ = md;
        }
    }
}

fn collect_calls(node: &SyntaxNode, calls: &mut Vec<(String, TopSubtree, TextRange)>) {
    for ev in node.preorder() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else { continue };
        if let Some(mc) = ast::MacroCall::cast(n.clone()) {
            let name = mc
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .map(|n| n.text().to_string());
            if let (Some(name), Some(tt)) = (name, mc.token_tree()) {
                let top = syntax_node_to_token_tree(
                    tt.syntax(),
                    FileSpanMap,
                    Span {
                        range: mc.syntax().text_range(),
                        anchor: SpanAnchor {
                            file_id: ra_ap_span::EditionedFileId::new(
                                ra_ap_span::FileId::from_raw(0),
                                Edition::CURRENT,
                            ),
                            ast_id: ROOT_ERASED_FILE_AST_ID,
                        },
                        ctx: SyntaxContext::root(Edition::CURRENT),
                    },
                    DocCommentDesugarMode::Mbe,
                );
                calls.push((name, top, mc.syntax().text_range()));
            }
        }
    }
}

pub struct ExpandOutcome {
    pub invocations: usize,
    pub expanded: usize,
    pub errors: usize,
    pub mapped_call_site: usize,
    pub mapped_def_site: usize,
    pub unmapped: usize,
    pub ms: u128,
    pub text: String,
}

pub fn expand_file(path: &str, _verbose: bool) -> Result<String, String> {
    let (row, _) = expand_file_to(path, std::path::Path::new(path).parent().and_then(|p| p.to_str()).unwrap_or("."))?;
    Ok(row)
}

pub fn expand_file_to(path: &str, out_dir: &str) -> Result<(String, String), String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let parsed = SourceFile::parse(&src, Edition::CURRENT);
    let mut defs = HashMap::new();
    collect_rules(&parsed.syntax_node(), &mut defs);
    let mut calls = Vec::new();
    collect_calls(&parsed.syntax_node(), &mut calls);

    let db = Db::default();
    let mut expanded_text = src.clone();
    let mut expanded = 0usize;
    let mut errors = 0usize;
    let mut mapped_call = 0usize;
    let mut mapped_def = 0usize;
    let mut unmapped = 0usize;
    let mut edits: Vec<(u32, u32, String)> = Vec::new();

    for (name, call_tt, call_range) in &calls {
        let Some((def_tt, def_range)) = defs.get(name) else { continue };
        let mac = DeclarativeMacro::parse_macro_rules(def_tt, edition);
        if let Some(_pe) = mac.err() {
            errors += 1;
            continue;
        }
        let res = mac.expand(&db, &call_tt, |_span| {}, ra_ap_mbe::MacroCallStyle::FnLike, FileSpanMap.span_for(*call_range));
        if res.err.is_some() {
            errors += 1;
        }
        let (top, _) = res.value;
        let (cs, ds) = classify_spans(&top, *call_range, *def_range);
        mapped_call += cs;
        mapped_def += ds;
        let (parsed2, _) = token_tree_to_syntax_node(&top, ra_ap_parser::TopEntryPoint::MacroItems, &mut edition);
        expanded += 1;
        edits.push((
            u32::from(call_range.start()),
            u32::from(call_range.end()),
            spaced_text(parsed2.syntax_node()),
        ));
    }
    // splice: replace each invocation's byte range with its expansion, reverse
    // order so earlier offsets stay valid
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end, text) in edits {
        expanded_text.replace_range(start as usize..end as usize, &text);
    }
    let ms = t0.elapsed().as_millis();

    let stem = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out".into());
    let out_path = format!("{}/{}.expanded.rs", out_dir, stem);
    std::fs::write(&out_path, &expanded_text).map_err(|e| e.to_string())?;

    let row = format!(
        "{}\tdefs={}\tinvocations={}\texpanded={}\texpand_errors={}\tspan_call_site={}\tspan_def_site={}\tspan_unmapped={}\tms={}",
        path, defs.len(), calls.len(), expanded, errors, mapped_call, mapped_def, unmapped, ms
    );
    Ok((row, out_path))
}

fn spaced_text(node: SyntaxNode) -> String {
    node.preorder_with_tokens()
        .filter_map(|ev| match ev {
            ra_ap_syntax::WalkEvent::Enter(e) => e.into_token(),
            _ => None,
        })
        .map(|t| t.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn classify_spans(top: &TopSubtree, call_range: TextRange, def_range: TextRange) -> (usize, usize) {
    let mut call_site = 0;
    let mut def_site = 0;
    fn visit(view: ra_ap_tt::TokenTreesView, call_range: TextRange, def_range: TextRange, cs: &mut usize, ds: &mut usize) {
        for t in view.iter_flat_tokens() {
            if let ra_ap_tt::TokenTree::Leaf(leaf) = t {
                let span = match leaf {
                    ra_ap_tt::Leaf::Literal(l) => l.span,
                    ra_ap_tt::Leaf::Ident(i) => i.span,
                    ra_ap_tt::Leaf::Punct(p) => p.span,
                };
                if def_range.contains_range(span.range) {
                    *ds += 1;
                } else if call_range.contains_range(span.range) {
                    *cs += 1;
                }
            }
        }
    }
    visit(top.token_trees(), call_range, def_range, &mut call_site, &mut def_site);
    (call_site, def_site)
}


pub fn dump_spans(path: &str, out: &mut String) -> Result<(), String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let parsed = SourceFile::parse(&src, Edition::CURRENT);
    let mut calls = Vec::new();
    collect_calls(&parsed.syntax_node(), &mut calls);
    for (_name, _tt, range) in calls {
        let _ = writeln!(out, "{}\t{}\t{}", path, u32::from(range.start()), u32::from(range.end()));
    }
    Ok(())
}
