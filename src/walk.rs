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
    let n = adj.len();
    debug_assert_eq!(halt.len(), n, "halt mask must cover every node");
    let mut out: Vec<(u32, u32)> = Vec::new();
    if starts.is_empty() {
        return out;
    }

    // Group start frontier nodes by tag. starts is small relative to the graph;
    // sort a copy so each tag's block is contiguous (stable per-tag BFS order).
    let mut by_tag = starts.to_vec();
    by_tag.sort_unstable();
    by_tag.dedup();

    // Generation-stamped visited: seen[node] == gen means "reached in this tag's
    // walk". gen starts at 1 so the zero-init array reads as unvisited.
    let mut seen = vec![0u32; n];
    let mut gen: u32 = 0;
    let mut queue: VecDeque<u32> = VecDeque::new();

    let mut i = 0;
    while i < by_tag.len() {
        let tag = by_tag[i].0;
        gen += 1;
        queue.clear();

        // Enqueue this tag's whole start frontier before draining, so a start
        // node reached from another start in the same tag isn't double-walked.
        while i < by_tag.len() && by_tag[i].0 == tag {
            let node = by_tag[i].1;
            if (node as usize) < n && seen[node as usize] != gen {
                seen[node as usize] = gen;
                out.push((tag, node));
                queue.push_back(node);
            }
            i += 1;
        }

        while let Some(mid) = queue.pop_front() {
            // A halt node is recorded (it was enqueued) but never expanded.
            if halt[mid as usize] {
                continue;
            }
            for &node in &adj[mid as usize] {
                if seen[node as usize] != gen {
                    seen[node as usize] = gen;
                    out.push((tag, node));
                    queue.push_back(node);
                }
            }
        }
    }

    out.sort_unstable();
    out
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
}
