//! `inlay.rs` — `textDocument/inlayHint` provider for `.sprf` files.
//!
//! On request, runs `host_parse → walk_program → expand` against a
//! seeded buffer probe sink. Each emit fires `Probe { span, cursor }`;
//! probes are grouped by op span, and one inlay (`→ N cursors`) is
//! emitted per span at its `hi` byte.
//!
//! The handler reuses byte-to-position math from `main.rs`. To avoid
//! exposing `byte_to_position` cross-module, the helper is duplicated
//! here as a small `pub(crate) fn`. If a third copy lands later,
//! refactor both into a shared `pos.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::{
    expand, BufferProbeSink, ExpandOpts, FactStore, MemFactStore,
    MemQueue, ProbeSink, QueueBackend,
};

use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, Position,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

/// Build inlay hints for one document. `text` is the full file contents;
/// `root_dir` seeds `LowerCtx::root` (relative path resolution for
/// source ops like `fs`/`glob`). The probe drives a one-shot expand —
/// no state escapes this call.
pub fn inlays_for(text: &str, root_dir: PathBuf) -> Vec<InlayHint> {
    // 1. parse
    let (program, _parse_diags) = host_parse(text);

    // 2. walk with a buffer probe attached to LowerCtx
    let probe: Arc<BufferProbeSink<Cursor>> = Arc::new(BufferProbeSink::new());
    let probe_dyn: Arc<dyn ProbeSink<Cursor>> = probe.clone();
    let store: Arc<dyn FactStore<Cursor>> =
        Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, root_dir).with_probe(probe_dyn);
    let (pipes, _walk_diags) = walk_program(&program, &reg, &mut ctx);

    // 3. expand each pipe with a fresh seed cursor
    for pipe in pipes {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let inst = pipe.into_instance();
        expand(
            &inst,
            queue,
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    // 4. group by span
    let probes = probe.drain();
    let mut by_span: HashMap<(u32, u32), u32> = HashMap::new();
    for p in &probes {
        *by_span.entry((p.span.lo, p.span.hi)).or_insert(0) += 1;
    }

    // 5. emit one inlay per span at its hi byte
    let mut out: Vec<InlayHint> = by_span.into_iter()
        .map(|((lo, hi), count)| {
            let pos = byte_to_position(text, hi as usize);
            InlayHint {
                position: pos,
                label:    InlayHintLabel::String(format!("→ {} cursors", count)),
                kind:     Some(InlayHintKind::TYPE),
                tooltip:  None,
                padding_left:  Some(true),
                padding_right: Some(false),
                text_edits:    None,
                data:          Some(serde_json::json!({ "lo": lo, "hi": hi })),
            }
        })
        .collect();

    // Stable order so editors/tests get deterministic output.
    out.sort_by_key(|h| (h.position.line, h.position.character));
    out
}

/// Mirror of the helper in `main.rs` (private there). Kept in sync by
/// hand; if a third call site lands, hoist to a shared module.
pub(crate) fn byte_to_position(src: &str, off: usize) -> Position {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let bytes = src.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let slice = &src[line_start..off];
    let col: u32 = if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|c| c.len_utf16() as u32).sum()
    };
    Position::new(line, col)
}

