/// The primitive's own unit exercise.
/// @recompute unguarded: the fixture graph is three nodes and never re-ticks.
pub fn embed_graph_probe(graph: &Graph) -> Vec<Row> {
    embed_graph(graph)
}
