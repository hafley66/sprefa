// guarded: the digest skip and the recompute sit in one function.
pub fn eval_node2vec_rule(graph: &Graph) -> Vec<Row> {
    if load_rel_digest("node2vec") == graph.digest() {
        return Vec::new();
    }
    embed_graph(graph)
}
