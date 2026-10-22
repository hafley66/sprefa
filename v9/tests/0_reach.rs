//! Each case: stored edges, then a run's edge set; the reach the dataflow keeps must equal the
//! oracle's closure of the run's edges.

use std::collections::BTreeSet;
use v9::_2_store::Edge;
use v9::{_3_reach, _4_oracle};

const CASES: &[(&str, &[Edge], &[Edge])] = &[
    ("cold chain", &[], &[(1, 2), (2, 3), (3, 4)]),
    ("cycle", &[], &[(1, 2), (2, 1), (2, 3)]),
    ("edge removed mid chain", &[(1, 2), (2, 3), (3, 4)], &[(1, 2), (3, 4)]),
    ("edge added joins chains", &[(1, 2), (3, 4)], &[(1, 2), (2, 3), (3, 4)]),
    ("cycle broken", &[(1, 2), (2, 1)], &[(1, 2)]),
    ("no change", &[(1, 2), (2, 3)], &[(1, 2), (2, 3)]),
];

#[test]
fn reach_after_each_run_equals_the_oracle() {
    for (name, stored, run) in CASES {
        let stored: BTreeSet<Edge> = stored.iter().copied().collect();
        let run: BTreeSet<Edge> = run.iter().copied().collect();
        let mut changes: Vec<(Edge, isize)> = run.difference(&stored).map(|e| (*e, 1)).collect();
        changes.extend(stored.difference(&run).map(|e| (*e, -1)));

        let mut kept = _4_oracle::closure(&stored);
        for (row, diff) in _3_reach::settle(stored.iter().copied().collect(), changes).reach {
            match diff {
                1 => assert!(kept.insert(row), "{name}: +1 on a present row {row:?}"),
                -1 => assert!(kept.remove(&row), "{name}: -1 on an absent row {row:?}"),
                other => panic!("{name}: diff {other} on {row:?}"),
            }
        }
        assert_eq!(kept, _4_oracle::closure(&run), "{name}");
    }
}
