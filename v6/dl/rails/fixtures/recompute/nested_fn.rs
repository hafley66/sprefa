// A nested NAMED function owns its own marker: the outer guard does not clear
// the inner recompute, which is what the innermost-enclosing rule decides.
pub fn outer_guarded(graph: &Graph) -> Vec<Row> {
    fn inner_recompute(graph: &Graph) -> Vec<Row> {
        embed_graph(graph)
    }
    if load_rel_digest("node2vec") == graph.digest() {
        return Vec::new();
    }
    inner_recompute(graph)
}
