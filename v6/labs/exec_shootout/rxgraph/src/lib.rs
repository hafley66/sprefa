//! rx operator graph engine for the exec_shootout reachability workload.
//! Boxed operator trait objects wired at startup; the measured layer is the dynamic dispatch.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// One tuple on the wire, both ids are u32 per the IO contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Row {
    pub from: u32,
    pub to: u32,
}

/// A batch of tuples moving along an edge, tagged with its semi-naive round.
#[derive(Clone, Debug)]
pub struct Batch {
    pub round: u64,
    pub rows: Vec<Row>,
}

/// A boxed operator. Every operator consumes one input batch and appends
/// zero or more output batches. Downstream routing is owned by the driver.
pub trait Operator {
    fn name(&self) -> &'static str;
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>);
}

/// Map: apply a unary transform to every row.
pub struct MapOp<F> {
    pub name: &'static str,
    pub map: F,
}

impl<F: Fn(Row) -> Row> Operator for MapOp<F> {
    fn name(&self) -> &'static str {
        self.name
    }
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>) {
        let rows: Vec<Row> = input.rows.iter().map(|row| (self.map)(*row)).collect();
        emissions.push(Batch {
            round: input.round,
            rows,
        });
    }
}

/// Filter: keep only the rows a predicate accepts.
pub struct FilterOp<F> {
    pub name: &'static str,
    pub predicate: F,
}

impl<F: Fn(&Row) -> bool> Operator for FilterOp<F> {
    fn name(&self) -> &'static str {
        self.name
    }
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>) {
        let rows: Vec<Row> = input
            .rows
            .iter()
            .filter(|row| (self.predicate)(row))
            .copied()
            .collect();
        emissions.push(Batch {
            round: input.round,
            rows,
        });
    }
}

/// Distinct: emit only rows absent from the shared seen set, so each output is globally new.
#[derive(Clone)]
pub struct DistinctOp {
    pub name: &'static str,
    pub seen: Rc<RefCell<HashSet<(u32, u32)>>>,
}

impl Operator for DistinctOp {
    fn name(&self) -> &'static str {
        self.name
    }
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>) {
        let mut kept = Vec::new();
        {
            let mut seen = self.seen.borrow_mut();
            for row in &input.rows {
                if seen.insert((row.from, row.to)) {
                    kept.push(*row);
                }
            }
        }
        // An empty batch is the semi-naive stop signal: the caller drops it
        // rather than routing it onward.
        emissions.push(Batch {
            round: input.round,
            rows: kept,
        });
    }
}

/// Join reachable(from, to) against edge(to, target) keyed on the shared position.
/// The right edge index is fixed; only the left delta flows through the box.
pub struct JoinOp {
    pub name: &'static str,
    pub right: HashMap<u32, Vec<u32>>,
}

impl Operator for JoinOp {
    fn name(&self) -> &'static str {
        self.name
    }
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>) {
        let mut rows = Vec::new();
        for row in &input.rows {
            if let Some(targets) = self.right.get(&row.to) {
                for target in targets {
                    rows.push(Row {
                        from: row.from,
                        to: *target,
                    });
                }
            }
        }
        emissions.push(Batch {
            round: input.round,
            rows,
        });
    }
}

#[derive(Default)]
pub struct SinkState {
    pub derived: u64,
    pub checksum_fold: u64,
}

/// Sink: terminal node in the graph. Accumulates the derived count and the
/// checksum from every row it receives, and emits nothing onward.
pub struct SinkOp {
    pub name: &'static str,
    pub state: Rc<RefCell<SinkState>>,
}

impl SinkOp {
    pub fn state(&self) -> Rc<RefCell<SinkState>> {
        Rc::clone(&self.state)
    }
}

pub fn pair_checksum(from: u32, to: u32) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&from.to_le_bytes());
    bytes[4..].copy_from_slice(&to.to_le_bytes());
    fnv1a64(&bytes)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl Operator for SinkOp {
    fn name(&self) -> &'static str {
        self.name
    }
    fn push(&mut self, input: &Batch, emissions: &mut Vec<Batch>) {
        let _ = emissions;
        let mut state = self.state.borrow_mut();
        for row in &input.rows {
            state.derived += 1;
            state.checksum_fold ^= pair_checksum(row.from, row.to);
        }
    }
}

/// A single node in the operator graph: the boxed operator plus the ids of
/// the nodes its emissions are routed to.
struct Node {
    op: Box<dyn Operator>,
    downstream: Vec<usize>,
}

struct Pending {
    node: usize,
    batch: Batch,
}

/// Program: a graph of boxed operators plus a driver routing batches until the
/// fixpoint delta is empty, the semi-naive stop.
pub struct Program {
    nodes: Vec<Node>,
    seed_node: usize,
    fixpoint_node: usize,
    sink_state: Rc<RefCell<SinkState>>,
}

pub struct RunReport {
    pub derived: u64,
    pub checksum: u64,
    pub max_round: u64,
    pub operator_pushes: u64,
}

impl Program {
    pub fn run(&mut self, seed: Vec<Row>) -> RunReport {
        let mut queue: VecDeque<Pending> = VecDeque::new();
        queue.push_back(Pending {
            node: self.seed_node,
            batch: Batch {
                round: 0,
                rows: seed,
            },
        });

        let mut operator_pushes = 0u64;
        let mut max_round = 0u64;

        while let Some(pending) = queue.pop_front() {
            let downstream = self.nodes[pending.node].downstream.clone();
            let mut emissions = Vec::new();
            self.nodes[pending.node]
                .op
                .push(&pending.batch, &mut emissions);
            operator_pushes += 1;
            if pending.batch.round > max_round {
                max_round = pending.batch.round;
            }
            for emission in emissions {
                // An empty batch does no work and closes a fixpoint sweep, so
                // it is never routed onward, which stops the feedback loop.
                if emission.rows.is_empty() {
                    continue;
                }
                for downstream_node in &downstream {
                    // Each pass through the fixpoint node starts the next
                    // semi-naive round.
                    let round = if pending.node == self.fixpoint_node {
                        pending.batch.round + 1
                    } else {
                        emission.round
                    };
                    queue.push_back(Pending {
                        node: *downstream_node,
                        batch: Batch {
                            round,
                            rows: emission.rows.clone(),
                        },
                    });
                }
            }
        }

        let report = RunReport {
            derived: self.sink_state.borrow().derived,
            checksum: self.sink_state.borrow().checksum_fold,
            max_round,
            operator_pushes,
        };
        report
    }
}

/// Build the reachability program over the given edges.
/// Wiring: seed -> [join, sink], join -> [fixpoint], fixpoint -> [join, sink].
pub fn build_reachability(edges: &[Row]) -> Program {
    let shared_seen: Rc<RefCell<HashSet<(u32, u32)>>> = Rc::new(RefCell::new(HashSet::default()));

    let mut right: HashMap<u32, Vec<u32>> = HashMap::default();
    for edge in edges {
        right.entry(edge.from).or_default().push(edge.to);
    }

    let seed_node = 0usize;
    let join_node = seed_node + 1;
    let fixpoint_node = seed_node + 2;
    let sink_node = seed_node + 3;

    let seed_distinct = DistinctOp {
        name: "seed_distinct",
        seen: Rc::clone(&shared_seen),
    };
    let join = JoinOp {
        name: "join",
        right,
    };
    let fixpoint_distinct = DistinctOp {
        name: "fixpoint_distinct",
        seen: shared_seen,
    };
    let mut nodes = Vec::new();
    nodes.push(Node {
        op: Box::new(seed_distinct),
        downstream: vec![join_node, sink_node],
    });
    nodes.push(Node {
        op: Box::new(join),
        downstream: vec![fixpoint_node],
    });
    nodes.push(Node {
        op: Box::new(fixpoint_distinct),
        downstream: vec![join_node, sink_node],
    });

    let sink_state: Rc<RefCell<SinkState>> = Rc::new(RefCell::new(SinkState::default()));
    let sink = SinkOp {
        name: "sink",
        state: Rc::clone(&sink_state),
    };
    nodes.push(Node {
        op: Box::new(sink),
        downstream: vec![],
    });

    Program {
        nodes,
        seed_node,
        fixpoint_node,
        sink_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(nodes: u32) -> Vec<Row> {
        let mut rows = Vec::new();
        for node in 0..nodes - 1 {
            rows.push(Row {
                from: node,
                to: node + 1,
            });
        }
        rows
    }

    #[test]
    fn semi_naive_stops_on_empty_delta() {
        // A 4-node chain reaches its full closure, C(4,2) = 6 pairs, in two
        // semi-naive rounds; the empty delta then drains the queue.
        let mut program = build_reachability(&chain(4));
        let report = program.run(chain(4));
        assert_eq!(report.derived, 6);
        assert_eq!(report.max_round, 2);
        assert_eq!(report.operator_pushes, 9);
    }

    #[test]
    fn cycles_terminate() {
        // A 2-node cycle has four finite reachable pairs; the shared distinct
        // set stops the fixpoint from looping.
        let cycle = vec![Row { from: 0, to: 1 }, Row { from: 1, to: 0 }];
        let mut program = build_reachability(&cycle);
        let report = program.run(cycle);
        assert_eq!(report.derived, 4);
        assert_eq!(report.operator_pushes, 7);
    }

    #[test]
    fn checksum_matches_hand_computed_value() {
        // Chain 0 -> 1 -> 2 -> 3: six derived pairs whose checksum XOR was
        // computed by hand as 0e0086019623ec40.
        let mut program = build_reachability(&chain(4));
        let report = program.run(chain(4));
        assert_eq!(report.checksum, 0x0e00_8601_9623_ec40);
        assert_eq!(report.derived, 6);
    }

    #[test]
    fn map_and_filter_operators_flow() {
        let mut map = MapOp {
            name: "map",
            map: |row: Row| Row {
                from: row.from + 10,
                to: row.to,
            },
        };
        let mut filter = FilterOp {
            name: "filter",
            predicate: |row: &Row| row.from > 10,
        };
        let input = Batch {
            round: 0,
            rows: vec![Row { from: 0, to: 1 }, Row { from: 1, to: 2 }],
        };
        let mut emissions = Vec::new();
        map.push(&input, &mut emissions);
        assert_eq!(emissions.len(), 1);
        assert_eq!(emissions[0].rows[0].from, 10);

        let mut filtered = Vec::new();
        filter.push(&emissions[0], &mut filtered);
        assert_eq!(filtered[0].rows.len(), 1);
        assert_eq!(filtered[0].rows[0].from, 11);
    }
}
