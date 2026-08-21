// The v5 misfire: a closure sits between the guard and the recompute. v5's
// nearest-decl-above model resolved them to two different enclosing decls.
pub fn eval_with_closure(graph: &Graph) -> Vec<Row> {
    if load_rel_digest("node2vec") == graph.digest() {
        return Vec::new();
    }
    let score = |row: &Row| row.weight * 2;
    let ranked: Vec<i64> = graph.rows().iter().map(score).collect();
    let _ = ranked;
    embed_graph(graph)
}
