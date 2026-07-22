//! dd differential-dataflow reach oracle — its OWN test binary (separate
//! process) so timely's runtime + memory pressure never shares a process with
//! the SQLite engine tests. Sharing races the global `stmt_counter` and trips
//! the memcap under parallel execution. See ../AGENTS.md.

use sprefa_store::oracle::dd::{mix, DdBfs};

#[test]
fn test_dd_simple_reach() {
    // 0 -> 1 -> 2 -> 3, and 1 -> 4; root 0 => reachable {0,1,2,3,4}
    let edges = vec![(0, 1), (1, 2), (2, 3), (1, 4)];
    let mut oracle = DdBfs::new();
    oracle.setup(0, &edges);

    let (digest, card) = oracle.reachable();
    assert_eq!(card, 5, "Expected 5 reachable nodes (0, 1, 2, 3, 4)");
    let expected_digest = mix(0) ^ mix(1) ^ mix(2) ^ mix(3) ^ mix(4);
    assert_eq!(digest, expected_digest, "Digest mismatch");
}

#[test]
fn test_dd_add_edge() {
    // 0 -> 1 (reachable {0,1}); add 1 -> 2 (reachable {0,1,2})
    let mut oracle = DdBfs::new();
    oracle.setup(0, &[(0, 1)]);
    let (_, card1) = oracle.reachable();
    assert_eq!(card1, 2, "Initially 2 reachable nodes");

    oracle.add_edge(1, 2);
    let (_, card2) = oracle.reachable();
    assert_eq!(card2, 3, "After adding edge, 3 reachable nodes");
}

#[test]
fn test_dd_del_edge() {
    // 0 -> 1, 1 -> 2 (reachable {0,1,2}); del 1 -> 2 (reachable {0,1})
    let mut oracle = DdBfs::new();
    oracle.setup(0, &[(0, 1), (1, 2)]);
    let (_, card1) = oracle.reachable();
    assert_eq!(card1, 3, "Initially 3 reachable nodes");

    oracle.del_edge(1, 2);
    let (_, card2) = oracle.reachable();
    assert_eq!(card2, 2, "After deleting edge, 2 reachable nodes");
}

#[test]
fn test_dd_batch_updates() {
    // 0 -> 1; batch add 0 -> 2, del 0 -> 1 => still 2 reachable
    let mut oracle = DdBfs::new();
    oracle.setup(0, &[(0, 1)]);
    let (_, card1) = oracle.reachable();
    assert_eq!(card1, 2);

    oracle.batch(&[(0, 2)], &[(0, 1)]);
    let (_, card2) = oracle.reachable();
    assert_eq!(card2, 2, "After batch (add 0->2, del 0->1), still 2 reachable");
}
