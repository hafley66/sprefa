//! Clone-proposer relations: `propose_extract`, `propose_clone`.

use anyhow::Result;
use std::collections::HashMap;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::{read_content, Engine};
use crate::lower::txt_tbl;
use crate::scip_import;

use super::{col, RelKind};

// --- propose_extract (clone proposer) ----------------------------------------

/// Extract-function proposals: one row `(path, lo, hi, param)` per free var of
/// each verbatim-duplicated block found in a scanned Rust file. `lo`/`hi` bound
/// the block's first occurrence (1-based lines); the param set is the inferred
/// extract-fn signature (free vars = read in the block, not bound inside it).
/// Whole-corpus: recompute all, compare to stored, early-out if equal. Reuses
/// `node_file_set` for the file list and `propose::extract_proposals`.
pub struct ProposeExtractKind;

impl RelKind for ProposeExtractKind {
    fn rels(&self) -> &'static [&'static str] {
        &["propose_extract"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "propose_extract".into(),
            cols: vec![
                col("path", Type::Path),
                col("lo", Type::Int),
                col("hi", Type::Int),
                col("param", Type::Text),
            ],
            group: "propose",
            doc: "proposed extract-function refactor spans (path, lo, hi, param)",
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in extract-proposal relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let roots = eng.repo_roots();
        let root = eng.root.clone();
        let files = eng.node_file_set(None)?;
        let mut computed: Vec<(String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") {
                continue;
            }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            for prop in crate::propose::extract_proposals(&content) {
                for p in prop.params {
                    computed.push((path.clone(), prop.lo as i64, prop.hi as i64, p));
                }
            }
        }
        computed.sort();
        computed.dedup();
        let stored: Vec<(String, i64, i64, String)> = eng.db.query_rows(
            "propose_extract",
            &format!(
                "SELECT \"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"path\",\"lo\",\"hi\",\"param\"",
                txt_tbl("propose_extract")
            ),
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?, r.get::<_, String>(3)?,
            )),
        )?;
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(p, lo, hi, pm)| {
                vec![
                    Value::Text(p),
                    Value::Int(lo),
                    Value::Int(hi),
                    Value::Text(pm),
                ]
            })
            .collect();
        eng.refresh_rel("propose_extract", &["path", "lo", "hi", "param"], &rows)?;
        Ok(true)
    }
}

// --- propose_clone (multi-kernel clone proposer) -----------------------------

/// Multi-kernel clone-detection relation: `propose_clone(kernel, path, lo, hi,
/// param)`. Runs all 9 clone-detection kernels (verbatim, ast, tree, cfg, ddg,
/// cgraph, ngram, symbol, call) on every scanned Rust file; `kernel` selects the
/// detector. Symbol and call-seq kernels need `index.scip`; they emit no rows if
/// the index is absent.
pub struct ProposeCloneKind;

impl RelKind for ProposeCloneKind {
    fn rels(&self) -> &'static [&'static str] {
        &["propose_clone"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "propose_clone".into(),
            cols: vec![
                col("kernel", Type::Text),
                col("path", Type::Path),
                col("lo", Type::Int),
                col("hi", Type::Int),
                col("param", Type::Text),
            ],
            group: "propose",
            doc: "proposed clone/near-duplicate groups keyed by a shared kernel",
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in clone-detection relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let roots = eng.repo_roots();
        let root = eng.root.clone();
        let files = eng.node_file_set(None)?;
        let scip_spans: HashMap<String, Vec<(i32, i32, String)>> =
            if let Some(idx) = scip_import::index_path(&root) {
                match scip_import::load(&idx, &root, &eng.self_slug()) {
                    Ok(rows) => {
                        let mut map: HashMap<String, Vec<(i32, i32, String)>> = HashMap::new();
                        for (file, l, c, sym) in rows.occ_spans {
                            map.entry(file).or_default().push((l, c, sym));
                        }
                        map
                    }
                    Err(_) => HashMap::new(),
                }
            } else {
                HashMap::new()
            };
        let mut computed: Vec<(String, String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") {
                continue;
            }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            let spans_owned = scip_spans.get(&path).cloned().unwrap_or_default();
            let spans: Vec<(i32, i32, &str)> = spans_owned
                .iter()
                .map(|(l, c, s)| (*l, *c, s.as_str()))
                .collect();
            let kernels: Vec<(&str, Vec<crate::propose::Proposal>)> = vec![
                ("verbatim", crate::propose::extract_proposals(&content)),
                ("ast", crate::propose::ast_shape_proposals(&content)),
                ("tree", crate::propose::tree_shape_proposals(&content)),
                ("cfg", crate::propose::cfg_shape_proposals(&content)),
                ("ddg", crate::propose::ddg_shape_proposals(&content)),
                (
                    "cgraph",
                    crate::propose::callgraph_shape_proposals(&content),
                ),
                ("ngram", crate::propose::ngram_stat_proposals(&content)),
                (
                    "symbol",
                    crate::propose::symbol_shape_proposals(&content, &spans),
                ),
                ("call", crate::propose::call_seq_proposals(&content, &spans)),
            ];
            for (kname, props) in kernels {
                for prop in props {
                    for p in &prop.params {
                        computed.push((
                            kname.to_string(),
                            path.clone(),
                            prop.lo as i64,
                            prop.hi as i64,
                            p.clone(),
                        ));
                    }
                }
            }
        }
        computed.sort();
        computed.dedup();
        let stored: Vec<(String, String, i64, i64, String)> = eng.db.query_rows(
            "propose_clone",
            &format!(
                "SELECT \"kernel\",\"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"kernel\",\"path\",\"lo\",\"hi\",\"param\"",
                txt_tbl("propose_clone")
            ),
            &[],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            },
        )?;
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(k, p, lo, hi, pm)| {
                vec![
                    Value::Text(k),
                    Value::Text(p),
                    Value::Int(lo),
                    Value::Int(hi),
                    Value::Text(pm),
                ]
            })
            .collect();
        eng.refresh_rel(
            "propose_clone",
            &["kernel", "path", "lo", "hi", "param"],
            &rows,
        )?;
        Ok(true)
    }
}
