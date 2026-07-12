//! Multi-source, halt-gated forward BFS. Pure graph: no SQL, no engine types.
//!
//! The contraction walk behind `port_reach`: from each tagged seed, walk `edge`
//! forward, RECORDING every node reached but never expanding OUT of a halt node
//! (a node still gets recorded when reached — it just stops being a stepping
//! stone). Answers "which nodes does each port reach without passing through
//! another port" in one in-memory pass instead of a SQLite semi-naive fixpoint.
//!
//! The `.dl` shape this executes (see std flow-panel's `port_reach`):
//!
//!   head(tag, node) <- seed(tag, start), edge(start, node).       # start frontier
//!   head(tag, node) <- head(tag, mid), !halt(mid, _), edge(mid, node).
//!
//! The seed rules already take ONE `edge` hop before the recursion, so the BFS
//! start frontier is the one-hop-out of each seed's start node, matching the
//! seed rules exactly.
//!
//! Cycles in `edge` are handled by the per-tag visited set (no depth cap needed;
//! the SQL form capped at 64 only because a fixpoint over a cyclic graph would
//! otherwise not converge without the lattice — the visited set converges here).

use std::collections::VecDeque;

/// The general walk behind every reachability rule the recognizer handles:
/// multi-source forward BFS carrying a per-node DEPTH, with an optional stop
/// rule (`halt`) and an optional expansion cap (`depth_cap`). Returns the
/// deduped `(tag, node, min_depth)` set — each node recorded once per tag at the
/// shortest depth it's reached (BFS layer = min depth, which is exactly what a
/// `merge(MinBy(depth))` lattice computes, for free).
///
/// - `starts`: `(tag, node, depth)` frontier — the base rules' output. Sorted so
///   that within a tag the first-seen occurrence of a node carries its minimum
///   start depth. (All current rules seed at depth 0; a future mixed-depth seed
///   would want a bucket queue instead of FIFO — see the plan.)
/// - `halt`: `Some(mask)` stops expansion at any true node (still recorded);
///   `None` = no stop rule.
/// - `depth_cap`: `Some(cap)` = a node at depth >= cap is recorded but not
///   expanded (the `d0 < cap` guard); `None` = expand until the visited set
///   closes (cycles terminate via the per-tag visited stamp).
pub fn multi_source_walk(
    adj: &[Vec<u32>],
    starts: &[(u32, u32, i64)],
    halt: Option<&[bool]>,
    depth_cap: Option<i64>,
) -> Vec<(u32, u32, i64)> {
    let n = adj.len();
    if let Some(h) = halt { debug_assert_eq!(h.len(), n, "halt mask must cover every node"); }
    let mut out: Vec<(u32, u32, i64)> = Vec::new();
    if starts.is_empty() { return out; }

    // Sort so each tag's block is contiguous and, within a tag, a node's first
    // occurrence carries its minimum start depth.
    let mut by_tag = starts.to_vec();
    by_tag.sort_unstable();
    by_tag.dedup();

    let mut seen = vec![0u32; n];
    let mut gen: u32 = 0;
    let mut queue: VecDeque<(u32, i64)> = VecDeque::new();

    let mut i = 0;
    while i < by_tag.len() {
        let tag = by_tag[i].0;
        gen += 1;
        queue.clear();
        while i < by_tag.len() && by_tag[i].0 == tag {
            let (_, node, depth) = by_tag[i];
            if (node as usize) < n && seen[node as usize] != gen {
                seen[node as usize] = gen;
                out.push((tag, node, depth));
                queue.push_back((node, depth));
            }
            i += 1;
        }
        while let Some((mid, d)) = queue.pop_front() {
            if let Some(h) = halt { if h[mid as usize] { continue; } }
            if let Some(cap) = depth_cap { if d >= cap { continue; } }
            for &node in &adj[mid as usize] {
                if seen[node as usize] != gen {
                    seen[node as usize] = gen;
                    out.push((tag, node, d + 1));
                    queue.push_back((node, d + 1));
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// For each tag, forward-BFS over `adj` from that tag's start frontier, halting
/// expansion at any node marked in `halt`. Returns the deduped, sorted set of
/// `(tag, reached_node)` id pairs.
///
/// - `adj`: adjacency, `adj[u]` = out-neighbors of node `u`.
/// - `starts`: `(tag, start_node)` frontier pairs — already the one-hop image of
///   the seed rules (caller computes `seed ⋈ edge`). Each `start_node` is
///   recorded as reached by `tag` and, if not a halt node, expanded.
/// - `halt`: `halt[u]` true means node `u` stops expansion (still recorded).
///
/// Per-tag visited is tracked by a generation stamp array (`seen[node] == gen`)
/// so resetting between tags is O(1) — no per-tag bitset clear over the whole
/// node set, which would be O(tags x nodes).
pub fn multi_source_halt_bfs(
    adj: &[Vec<u32>],
    starts: &[(u32, u32)],
    halt: &[bool],
) -> Vec<(u32, u32)> {
    // Halt-only, depth-agnostic view of `multi_source_walk`: seed every start at
    // depth 0, no cap, drop the returned depth.
    let starts3: Vec<(u32, u32, i64)> = starts.iter().map(|&(tag, node)| (tag, node, 0)).collect();
    multi_source_walk(adj, &starts3, Some(halt), None)
        .into_iter().map(|(tag, node, _)| (tag, node)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build adjacency from (from, to) edges over `n` nodes.
    fn adj_of(n: usize, edges: &[(u32, u32)]) -> Vec<Vec<u32>> {
        let mut adj = vec![Vec::new(); n];
        for &(from, to) in edges {
            adj[from as usize].push(to);
        }
        adj
    }

    // Linear chain 0->1->2->3, no halts, one seed frontier node {1} tagged 0.
    // BFS records the start (1) and everything downstream: 1,2,3.
    #[test]
    fn linear_chain_records_start_and_downstream() {
        let adj = adj_of(4, &[(0, 1), (1, 2), (2, 3)]);
        let halt = vec![false; 4];
        let got = multi_source_halt_bfs(&adj, &[(0, 1)], &halt);
        assert_eq!(got, vec![(0, 1), (0, 2), (0, 3)]);
    }

    // Halt at node 2: it is recorded when reached but the walk does not expand
    // past it, so 3 is never reached.
    #[test]
    fn halt_node_is_recorded_but_not_expanded() {
        let adj = adj_of(4, &[(0, 1), (1, 2), (2, 3)]);
        let mut halt = vec![false; 4];
        halt[2] = true;
        let got = multi_source_halt_bfs(&adj, &[(0, 1)], &halt);
        assert_eq!(got, vec![(0, 1), (0, 2)], "3 is behind the halt node 2");
    }

    // A cycle 1->2->3->1 with no halt must terminate via the visited set and
    // record each node once (no depth cap, no runaway).
    #[test]
    fn cycle_terminates_and_dedups() {
        let adj = adj_of(4, &[(1, 2), (2, 3), (3, 1)]);
        let halt = vec![false; 4];
        let got = multi_source_halt_bfs(&adj, &[(7, 1)], &halt);
        assert_eq!(got, vec![(7, 1), (7, 2), (7, 3)]);
    }

    // Two tags over the same graph keep separate reach sets; the shared node is
    // recorded once per tag, not merged.
    #[test]
    fn two_tags_keep_separate_reach_sets() {
        // 0->2, 1->2, 2->3
        let adj = adj_of(4, &[(0, 2), (1, 2), (2, 3)]);
        let halt = vec![false; 4];
        // tag 10 starts at 0, tag 20 starts at 1; both reach 2 then 3.
        let got = multi_source_halt_bfs(&adj, &[(10, 0), (20, 1)], &halt);
        assert_eq!(
            got,
            vec![(10, 0), (10, 2), (10, 3), (20, 1), (20, 2), (20, 3)]
        );
    }

    // Multiple start-frontier nodes for one tag (the seed rule matched several
    // one-hop targets) are all recorded and expanded under that tag.
    #[test]
    fn multiple_starts_one_tag() {
        // 1->4, 2->5, 5->6
        let adj = adj_of(7, &[(1, 4), (2, 5), (5, 6)]);
        let halt = vec![false; 7];
        let got = multi_source_halt_bfs(&adj, &[(0, 1), (0, 2)], &halt);
        assert_eq!(got, vec![(0, 1), (0, 2), (0, 4), (0, 5), (0, 6)]);
    }

    // A start node that is itself a halt node: recorded, not expanded.
    #[test]
    fn start_that_is_halt_is_recorded_only() {
        let adj = adj_of(3, &[(0, 1), (1, 2)]);
        let mut halt = vec![false; 3];
        halt[0] = true;
        let got = multi_source_halt_bfs(&adj, &[(9, 0)], &halt);
        assert_eq!(got, vec![(9, 0)], "start 0 halts immediately");
    }

    #[test]
    fn empty_starts_is_empty() {
        let adj = adj_of(2, &[(0, 1)]);
        let halt = vec![false; 2];
        assert!(multi_source_halt_bfs(&adj, &[], &halt).is_empty());
    }

    // --- depth-carrying walk (multi_source_walk) ------------------------------

    // Chain 0->1->2->3 seeded at 0 depth 0: BFS layer == depth.
    #[test]
    fn walk_depth_is_bfs_layer() {
        let adj = adj_of(4, &[(0, 1), (1, 2), (2, 3)]);
        let got = multi_source_walk(&adj, &[(0, 0, 0)], None, None);
        assert_eq!(got, vec![(0, 0, 0), (0, 1, 1), (0, 2, 2), (0, 3, 3)]);
    }

    // Depth cap: `d0 < cap` means a node AT depth cap is recorded but not
    // expanded. cap=2 on the chain records depths 0,1,2 and stops (3 unreached).
    #[test]
    fn walk_depth_cap_records_at_cap_but_stops() {
        let adj = adj_of(4, &[(0, 1), (1, 2), (2, 3)]);
        let got = multi_source_walk(&adj, &[(0, 0, 0)], None, Some(2));
        assert_eq!(got, vec![(0, 0, 0), (0, 1, 1), (0, 2, 2)], "node 3 is past the cap");
    }

    // Two paths to the same node — a short (2 hops) and a long (3 hops): the
    // visited set keeps the MIN depth (what merge(MinBy(d)) computes).
    #[test]
    fn walk_keeps_min_depth_when_two_paths() {
        // 0->1->4 (depth 2) and 0->2->3->4 (depth 3)
        let adj = adj_of(5, &[(0, 1), (1, 4), (0, 2), (2, 3), (3, 4)]);
        let got = multi_source_walk(&adj, &[(0, 0, 0)], None, None);
        // node 4 reached at min depth 2, not 3.
        assert!(got.contains(&(0, 4, 2)), "min-depth 2 expected, got {got:?}");
        assert!(!got.contains(&(0, 4, 3)), "the longer depth-3 path must not win");
    }

    // Cycle with a cap: terminates via the visited set, records min depth.
    #[test]
    fn walk_cycle_with_cap_terminates() {
        let adj = adj_of(3, &[(0, 1), (1, 2), (2, 0)]);
        let got = multi_source_walk(&adj, &[(0, 0, 0)], None, Some(64));
        assert_eq!(got, vec![(0, 0, 0), (0, 1, 1), (0, 2, 2)]);
    }

    // Halt + depth together (the general case): halt at node 2, chain 0..3.
    #[test]
    fn walk_halt_and_depth() {
        let adj = adj_of(4, &[(0, 1), (1, 2), (2, 3)]);
        let mut halt = vec![false; 4];
        halt[2] = true;
        let got = multi_source_walk(&adj, &[(0, 0, 0)], Some(&halt), None);
        assert_eq!(got, vec![(0, 0, 0), (0, 1, 1), (0, 2, 2)], "3 is behind the halt at 2");
    }
}
