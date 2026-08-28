//! v5_normalize — the parity oracle. Runs v5's TS extraction (`TsTypes`) over a
//! file IN-PROCESS (no database, no repo: `TypeLang::extract` takes just
//! `(file, content)`) and emits the facts as canonical sorted lines — the same
//! shape v6's `golden_parity` test produces from its own `flatten`, so the two
//! sides diff cleanly. This is the "v5 extracted and cleaned up" ground truth.
//!
//! v6 is a deliberately-isolated workspace (its own `[workspace]`, no v5 in the
//! build graph), so parity is a CAPTURED ORACLE: run this once, commit its output
//! into v6's fixtures, and v6's test diffs against the committed baseline. v5 is
//! never a build dep of v6.
//!
//! Run from the repo root (the v5 crate):
//!   cargo run --example v5_normalize -- <path>
//! Capture the v6 baseline:
//!   cargo run --example v5_normalize -- \
//!     v6/sprefa-extract/tests/fixtures/ts/sample.ts \
//!     > v6/sprefa-extract/tests/fixtures/ts/sample.v5.jsonl
//!
//! Canonical line = tab-separated, sortable. Coordinates are the BEST v5 keeps:
//!   - type/call/const/doc families: 1-based LINE only (v5 drops the byte offset
//!     for these — `line_at` computes a line and discards the offset). v6 derives
//!     the same line from its byte `Span.start` (newline count + 1).
//!   - df: BYTE-EXACT. v5 keeps line + 0-based byte col, so byte_off =
//!     `line_starts[line-1] + col`, which equals the oxc expression start byte =
//!     v6's `Span.start`.
//!
//! Facets split into PORTED (v6 emits today; parity asserts empty diff) and
//! DEFERRED (v5-only by design; parity reports the counts as the migration
//! ledger). See v6's `golden_parity.rs` for the facet split + the waivers
//! (type_edge -> Resolve commit 4; const -> D-arrow-type ruling; df aux -> follow-up).

use std::collections::BTreeSet;

use sprefa_v5::graph::typegraph::{GoTypes, KotlinTypes, PyTypes, RustTypes, TsTypes, TypeLang};

fn main() {
    let path = std::env::args().nth(1).expect("usage: v5_normalize <path>");
    let content = std::fs::read_to_string(&path).expect("read");
    let starts = line_starts(&content);

    // Language is selected by extension (v5 `type_langs()` first-match). The
    // TypeFacts/CallFacts/DataflowFacts shapes are shared, so the canonical-line
    // emission below is language-agnostic; only the front-end differs (syn for
    // Rust, oxc for TS/JS, tree-sitter-go for Go, tree-sitter-kotlin-sg for
    // Kotlin). The `.kt`/`.kts` arm must precede the `.ts` fallback:
    // `"x.kts".ends_with(".ts")` is true (v5's type_langs ordering rule).
    let lang: &dyn TypeLang = if path.ends_with(".rs") {
        &RustTypes
    } else if path.ends_with(".go") {
        &GoTypes
    } else if path.ends_with(".kt") || path.ends_with(".kts") {
        &KotlinTypes
    } else if path.ends_with(".py") {
        &PyTypes
    } else {
        &TsTypes
    };

    let types = lang.extract(&path, &content);
    let calls = lang.extract_calls(&path, &content);
    let df = lang.extract_dataflow(&path, &content);

    let mut lines: BTreeSet<String> = BTreeSet::new();

    // ── PORTED: type entities + arrow sigs ──────────────────────────────────
    for entity in &types.entities {
        lines.insert(format!(
            "type_node\t{}\t{}\t{}",
            entity.kind.tag(),
            entity.name,
            entity.line
        ));
        if let Some(ty) = &entity.ty {
            for (pos, refs) in ty.params.iter().enumerate() {
                for r in refs {
                    lines.insert(format!(
                        "type_sig\t{}\tparam\t{}\t{}",
                        entity.line,
                        pos,
                        r.name()
                    ));
                }
            }
            for r in &ty.ret {
                lines.insert(format!("type_sig\t{}\tret\t0\t{}", entity.line, r.name()));
            }
        }
    }
    // ── DEFERRED v5-only: type edges (Resolve<TypeF>, commit 4) ─────────────
    for edge in &types.edges {
        lines.insert(format!(
            "type_edge\t{}\t{}\t{}",
            edge.from, edge.to, edge.kind
        ));
    }
    // ── const facet (PORTED): Const entities flow as type_node (kind=const) in
    // the loop above; const_value rows join to their owner via the owner's line
    // (v5 ConstValueFact.sym -> the owning entity's declaration line). ─────────
    let sym_line: std::collections::HashMap<&str, u32> = types
        .entities
        .iter()
        .map(|e| (e.sym.as_str(), e.line))
        .collect();
    for c in &types.consts {
        let owner_line = sym_line.get(c.sym.as_str()).copied().unwrap_or(0);
        lines.insert(format!(
            "const_value\t{owner_line}\t{}\t{}\t{}",
            c.field, c.kind, c.text
        ));
    }
    // ── DEFERRED v5-only: docs ──────────────────────────────────────────────
    for doc in &types.docs {
        lines.insert(format!("doc\t{}\t{}", doc.sym, doc.line));
    }

    // ── PORTED: call defs + sites ───────────────────────────────────────────
    for def in &calls.defs {
        lines.insert(format!(
            "call_def\t{}\t{}\t{}",
            def.kind.tag(),
            def.name,
            def.line
        ));
    }
    for site in &calls.sites {
        lines.insert(format!("call_site\t{}\t{}", site.callee, site.line));
    }

    // ── PORTED: df nodes + edges (byte-exact) ───────────────────────────────
    let node_bytes: Vec<u32> = df
        .nodes
        .iter()
        .map(|node| {
            let line_start = starts.get((node.line - 1) as usize).copied().unwrap_or(0);
            line_start + node.col
        })
        .collect();
    for (node, &byte_off) in df.nodes.iter().zip(node_bytes.iter()) {
        lines.insert(format!(
            "df_node\t{}\t{}\t{}",
            node.kind, node.var, byte_off
        ));
    }
    for edge in &df.edges {
        let from = node_bytes.get(edge.from as usize).copied().unwrap_or(0);
        let to = node_bytes.get(edge.to as usize).copied().unwrap_or(0);
        lines.insert(format!("df_edge\t{}\t{}", from, to));
    }
    // ── DEFERRED v5-only: df enrichment aux (labels, not graph) ─────────────
    for (node_idx, pos) in &df.param_pos {
        lines.insert(format!("df_param_pos\t{node_idx}\t{pos}"));
    }
    for (call_idx, pos, arg_idx) in &df.args {
        lines.insert(format!("df_args\t{call_idx}\t{pos}\t{arg_idx}"));
    }
    for (node_idx, field, val_idx) in &df.fields {
        lines.insert(format!("df_fields\t{node_idx}\t{field}\t{val_idx}"));
    }
    for (node_idx, text, kind) in &df.lits {
        lines.insert(format!("df_lits\t{node_idx}\t{kind}\t{text}"));
    }
    for lf in &df.loops {
        lines.insert(format!("df_loop\t{}\t{}", lf.file, lf.start));
    }
    for nf in &df.nests {
        lines.insert(format!("df_nest\t{}\t{}", nf.loop_id, nf.depth));
    }

    for line in &lines {
        println!("{line}");
    }

    eprintln!(
        "v5_normalize: {} lines (entities={} edges={} consts={} docs={} call_defs={} sites={} \
         df_nodes={} df_edges={})",
        lines.len(),
        types.entities.len(),
        types.edges.len(),
        types.consts.len(),
        types.docs.len(),
        calls.defs.len(),
        calls.sites.len(),
        df.nodes.len(),
        df.edges.len(),
    );
}

/// Byte offset of the start of each 1-based line: line N starts at `starts[N-1]`.
/// `line_starts[line-1] + col` reconstructs the oxc byte offset v5's `ts_push`
/// computed `(line, col)` from — so it round-trips to v6's `Span.start`.
fn line_starts(content: &str) -> Vec<u32> {
    let mut out = vec![0u32];
    for (byte_off, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            out.push((byte_off + 1) as u32);
        }
    }
    out
}
