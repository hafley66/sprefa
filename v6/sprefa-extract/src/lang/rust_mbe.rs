//! In-process `macro_rules!` expansion (`plans/extract-macro-lab-2026-08-29/PLAN.md`
//! Option 1): splice a local invocation's expansion into the file's own text.

use ra_ap_mbe::DeclarativeMacro;
use ra_ap_parser::Edition;
use ra_ap_span::{Span as RaSpan, SpanAnchor, SyntaxContext, ROOT_ERASED_FILE_AST_ID};
use ra_ap_syntax::{ast, ast::HasName, AstNode, SourceFile, SyntaxNode, TextRange};
use ra_ap_syntax_bridge::{
    syntax_node_to_token_tree, token_tree_to_syntax_node, DocCommentDesugarMode, SpanMapper,
};
use ra_ap_tt::TopSubtree;
use std::collections::HashMap;
use std::ops::Range;

use crate::types::Span;

// A macro that keeps minting more of itself, never a budget to raise.
const MAX_PASSES: u32 = 8;
const MAX_GROWTH_FACTOR: usize = 4;

/// `Verbatim` bytes translate 1:1 to the original file. `Macro` bytes were
/// minted by one invocation; the whole run collapses to that invocation's span.
enum Chunk {
    Verbatim { start: u32, end: u32, orig_start: u32 },
    Macro { start: u32, end: u32, origin: Span, name: String },
}

impl Chunk {
    fn start(&self) -> u32 {
        match self {
            Chunk::Verbatim { start, .. } | Chunk::Macro { start, .. } => *start,
        }
    }
    fn end(&self) -> u32 {
        match self {
            Chunk::Verbatim { end, .. } | Chunk::Macro { end, .. } => *end,
        }
    }
}

/// The spliced file plus enough of a map to report a gained def/site's span
/// as the original invocation's span (`map_span`).
pub struct Expanded {
    pub text: String,
    chunks: Vec<Chunk>,
    // A pass wanted to expand past MAX_PASSES or MAX_GROWTH_FACTORx bytes.
    pub budget_hit: bool,
}

impl Expanded {
    /// Translate a byte range of `self.text` to the original file: exact for a
    /// `Verbatim` range, the whole invocation span for a `Macro` range.
    pub fn map_span(&self, spliced: Range<u32>) -> Option<Span> {
        let idx = self
            .chunks
            .binary_search_by(|c| {
                if spliced.start < c.start() {
                    std::cmp::Ordering::Greater
                } else if spliced.start >= c.end() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let chunk = &self.chunks[idx];
        if spliced.end > chunk.end() {
            return None;
        }
        match chunk {
            Chunk::Verbatim {
                start, orig_start, ..
            } => Some(Span {
                start: orig_start + (spliced.start - start),
                len: spliced.end - spliced.start,
            }),
            Chunk::Macro { origin, .. } => Some(*origin),
        }
    }

    /// `true` for a spliced range born inside a macro expansion.
    pub fn is_macro_span(&self, spliced: Range<u32>) -> bool {
        self.chunks.iter().any(|c| {
            matches!(c, Chunk::Macro { .. }) && c.start() <= spliced.start && spliced.end <= c.end()
        })
    }

    /// One row per distinct invocation, deduped across nested chunks that
    /// share one origin (f3: `outer!`/`inner!` collapse to one row).
    pub fn macro_sites(&self) -> Vec<(Span, &str)> {
        let mut seen = Vec::new();
        for c in &self.chunks {
            let Chunk::Macro { origin, name, .. } = c else {
                continue;
            };
            if !seen.iter().any(|(s, _): &(Span, &str)| *s == *origin) {
                seen.push((*origin, name.as_str()));
            }
        }
        seen
    }
}

struct FileSpanMap;

impl SpanMapper for FileSpanMap {
    fn span_for(&self, range: TextRange) -> RaSpan {
        ra_span_at(range)
    }
}

fn ra_span_at(range: TextRange) -> RaSpan {
    RaSpan {
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

fn collect_rules(node: &SyntaxNode, defs: &mut HashMap<String, TopSubtree>) {
    for ev in node.preorder() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else {
            continue;
        };
        let Some(mr) = ast::MacroRules::cast(n) else {
            continue;
        };
        let (Some(name), Some(tt)) = (mr.name().map(|n| n.text().to_string()), mr.token_tree())
        else {
            continue;
        };
        let top = syntax_node_to_token_tree(
            tt.syntax(),
            FileSpanMap,
            ra_span_at(tt.syntax().text_range()),
            DocCommentDesugarMode::Mbe,
        );
        defs.insert(name, top);
    }
}

/// One `name!(...)` invocation as found in the current pass's text.
struct Invocation {
    name: String,
    call_tt: TopSubtree,
    range: TextRange,
}

fn collect_calls(node: &SyntaxNode, out: &mut Vec<Invocation>) {
    for ev in node.preorder() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else {
            continue;
        };
        let Some(mc) = ast::MacroCall::cast(n) else {
            continue;
        };
        let name = mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string());
        let (Some(name), Some(tt)) = (name, mc.token_tree()) else {
            continue;
        };
        let call_range = mc.syntax().text_range();
        let call_tt = syntax_node_to_token_tree(
            tt.syntax(),
            FileSpanMap,
            ra_span_at(call_range),
            DocCommentDesugarMode::Mbe,
        );
        out.push(Invocation {
            name,
            call_tt,
            range: call_range,
        });
    }
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

/// One pass over `text`: expand every LOCAL `macro_rules!` invocation found
/// there. A name with no local def (cross-file, builtin, derive) is untouched.
fn expand_pass(text: &str) -> Vec<(Range<u32>, String, String)> {
    let parsed = SourceFile::parse(text, Edition::CURRENT);
    let root = parsed.syntax_node();
    let mut defs = HashMap::new();
    collect_rules(&root, &mut defs);
    let mut calls = Vec::new();
    collect_calls(&root, &mut calls);

    let db = Db::default();
    let mut edits = Vec::new();
    for inv in &calls {
        let Some(def_tt) = defs.get(&inv.name) else {
            continue;
        };
        let mac = DeclarativeMacro::parse_macro_rules(def_tt, edition);
        if mac.err().is_some() {
            continue;
        }
        let res = mac.expand(
            &db,
            &inv.call_tt,
            |_span| {},
            ra_ap_mbe::MacroCallStyle::FnLike,
            ra_span_at(inv.range),
        );
        if res.err.is_some() {
            continue;
        }
        let (top, _) = res.value;
        let (parsed2, _) =
            token_tree_to_syntax_node(&top, ra_ap_parser::TopEntryPoint::MacroItems, &mut edition);
        edits.push((
            Range {
                start: u32::from(inv.range.start()),
                end: u32::from(inv.range.end()),
            },
            spaced_text(parsed2.syntax_node()),
            inv.name.clone(),
        ));
    }
    edits
}

/// Applies one pass's edits (byte ranges into `old_text`/`old_chunks`) to
/// build the next text and chunk generation.
fn apply_pass(
    old_text: &str,
    old_chunks: &[Chunk],
    mut edits: Vec<(Range<u32>, String, String)>,
) -> (String, Vec<Chunk>) {
    edits.sort_by_key(|(r, _, _)| r.start);
    let mut new_text = String::with_capacity(old_text.len());
    let mut new_chunks = Vec::new();
    let mut cursor: u32 = 0;

    let push_verbatim_range = |from: u32, to: u32, new_text: &mut String, new_chunks: &mut Vec<Chunk>| {
        if from >= to {
            return;
        }
        new_text.push_str(&old_text[from as usize..to as usize]);
        for c in old_chunks {
            let lo = c.start().max(from);
            let hi = c.end().min(to);
            if lo >= hi {
                continue;
            }
            let shift = new_text.len() as u32 - (to - from);
            let new_start = shift + (lo - from);
            let new_end = shift + (hi - from);
            match c {
                Chunk::Verbatim { orig_start, start, .. } => new_chunks.push(Chunk::Verbatim {
                    start: new_start,
                    end: new_end,
                    orig_start: orig_start + (lo - start),
                }),
                Chunk::Macro { origin, name, .. } => new_chunks.push(Chunk::Macro {
                    start: new_start,
                    end: new_end,
                    origin: *origin,
                    name: name.clone(),
                }),
            }
        }
    };

    for (range, replacement, name) in &edits {
        push_verbatim_range(cursor, range.start, &mut new_text, &mut new_chunks);
        let (origin, name) = invocation_origin(old_chunks, range.clone(), name);
        let start = new_text.len() as u32;
        new_text.push_str(replacement);
        new_chunks.push(Chunk::Macro {
            start,
            end: new_text.len() as u32,
            origin,
            name,
        });
        cursor = range.end;
    }
    push_verbatim_range(cursor, old_text.len() as u32, &mut new_text, &mut new_chunks);
    (new_text, new_chunks)
}

/// The (span, name) an invocation at `range` reports: its own verbatim
/// position and its own name, or whatever the enclosing macro chunk carries.
fn invocation_origin(old_chunks: &[Chunk], range: Range<u32>, own_name: &str) -> (Span, String) {
    for c in old_chunks {
        if c.start() <= range.start && range.end <= c.end() {
            return match c {
                Chunk::Verbatim { start, orig_start, .. } => (
                    Span {
                        start: orig_start + (range.start - start),
                        len: range.end - range.start,
                    },
                    own_name.to_string(),
                ),
                Chunk::Macro { origin, name, .. } => (*origin, name.clone()),
            };
        }
    }
    (
        Span {
            start: range.start,
            len: range.end - range.start,
        },
        own_name.to_string(),
    )
}

/// Expand every LOCAL `macro_rules!` invocation in `content` to a fixpoint.
/// `None` when there is nothing local to expand; `budget_hit` marks a cap stop.
pub fn expand_file(content: &str) -> Option<Expanded> {
    let first_pass = expand_pass(content);
    if first_pass.is_empty() {
        return None;
    }

    let mut chunks = vec![Chunk::Verbatim {
        start: 0,
        end: content.len() as u32,
        orig_start: 0,
    }];
    let (mut text, new_chunks) = apply_pass(content, &chunks, first_pass);
    chunks = new_chunks;
    let mut budget_hit = false;

    for _pass in 1..MAX_PASSES {
        if text.len() > content.len() * MAX_GROWTH_FACTOR {
            budget_hit = true;
            break;
        }
        let edits = expand_pass(&text);
        if edits.is_empty() {
            break;
        }
        let (next_text, next_chunks) = apply_pass(&text, &chunks, edits);
        if next_text.len() > content.len() * MAX_GROWTH_FACTOR {
            budget_hit = true;
            break;
        }
        text = next_text;
        chunks = next_chunks;
    }
    if !budget_hit && !expand_pass(&text).is_empty() {
        budget_hit = true;
    }

    Some(Expanded {
        text,
        chunks,
        budget_hit,
    })
}
