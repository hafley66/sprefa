//! Embedding-similarity relation: `similar`.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::{knn_rows, Engine};
use crate::lower::tbl;

use super::{col, RelKind};

// --- embed (embedding similarity) --------------------------------------------

/// Embedding-similarity relation. `similar(a, b, score)` is the top-k nearest
/// neighbors of each embedded interned string by cosine; `score` is an Int =
/// round(cosine * 1_000_000), so a `.dl` rule can threshold in Int-only value
/// space. Vectors are content-addressed: one row per (StringId, backend) in
/// `_embeddings`, so identical content embeds once. Lazy like the other
/// indexers; brute-force O(n^2) cosine over the embedded set (capped by
/// SPREFA_EMBED_MAX, default 4096); SPREFA_SIMILAR_K (default 8) sets neighbors
/// per row. The sqlite-vec ANN path is the scale follow-on.
pub struct EmbedKind;

impl RelKind for EmbedKind {
    fn rels(&self) -> &'static [&'static str] {
        &["similar"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "similar".into(),
            cols: vec![col("a", Type::Text), col("b", Type::Text), col("score", Type::Int)], group: "embed",
            doc: "content-addressed nearest-neighbor pairs from the embedding backend, with score", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in embedding-similarity relation (similar)"
    }
    /// Encode every interned `_strings` row lacking a vector for the active
    /// backend (embed-once per (StringId, backend)), then materialize `similar`.
    /// Returns true if the `similar` row set could have changed, false on the
    /// steady-state no-op so the derived rebuild stays scoped.
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let embedder = crate::embed::make(None)?;
        let backend = embedder.name().to_string();
        let max: usize = std::env::var("SPREFA_EMBED_MAX").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(4096);

        // Content with no vector for THIS backend. Capped: only the first `max`
        // un-embedded strings are encoded per tick (the rest catch up next tick).
        let to_embed: Vec<(String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(
                "SELECT s.id, s.content FROM _strings s
                 WHERE s.id != '0'
                   AND NOT EXISTS (SELECT 1 FROM _embeddings e
                                   WHERE e.sid = s.id AND e.backend = ?1)
                 LIMIT ?2")?;
            let v: Vec<(String, String)> = s.query_map(rusqlite::params![backend, max as i64], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };
        if !to_embed.is_empty() {
            let texts: Vec<&str> = to_embed.iter().map(|(_, c)| c.as_str()).collect();
            let vecs = embedder.encode(&texts)?;
            let dim = embedder.dim() as i64;
            // collect-then-flush: one insert_rows, never per-row (the spine rule).
            let mut rows: Vec<Vec<Value>> = Vec::with_capacity(vecs.len());
            for ((sid, _), mut v) in to_embed.iter().cloned().zip(vecs) {
                crate::embed::l2_normalize(&mut v);
                rows.push(vec![
                    Value::Text(sid), Value::Text(backend.clone()),
                    Value::Int(dim), Value::Text(crate::embed::encode_vec(&v))]);
            }
            eng.db.insert_rows("_embeddings", &["sid", "backend", "dim", "vec"], &rows)?;
        }

        // Steady state: no new content AND `similar` already built -> no recompute.
        let similar_rows: i64 = eng.db.conn().query_row(
            &format!("SELECT count(*) FROM {}", tbl("similar")), [], |r| r.get(0))?;
        if to_embed.is_empty() && similar_rows > 0 { return Ok(false); }

        refresh_similar_rel(eng, &backend, max)?;
        Ok(true)
    }
}

/// Materialize `similar(a, b, score)`: top-k cosine neighbors of each embedded
/// string for `backend`. Brute-force pairwise over the (capped) embedded pool;
/// vectors are L2-normalized at store time so cosine is a dot product. `score` =
/// round(cosine * 1e6) as Int. Shares the `knn_rows` chokepoint with node2vec.
fn refresh_similar_rel(eng: &Engine, backend: &str, max: usize) -> Result<()> {
    let k: usize = std::env::var("SPREFA_SIMILAR_K").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(8);
    let pool: Vec<(String, Vec<f32>)> = {
        let conn = eng.db.conn();
        let mut s = conn.prepare("SELECT sid, vec FROM _embeddings WHERE backend = ?1 LIMIT ?2")?;
        let v: Vec<(String, Vec<f32>)> = s.query_map(rusqlite::params![backend, max as i64], |r|
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|x| x.ok())
            .map(|(sid, txt): (String, String)| (sid, crate::embed::parse_vec(&txt)))
            .collect();
        v
    };
    if pool.len() > 2000 {
        eprintln!("[similar] brute-force KNN over {} vectors (O(n^2)); \
                   cap with SPREFA_EMBED_MAX or wire sqlite-vec", pool.len());
    }
    let rows = knn_rows(&pool, k);
    eng.refresh_rel("similar", &["a", "b", "score"], &rows)?;
    Ok(())
}
