//! Call edges in, transitive reach out. Stored edges replay at time 0, the run's edge changes
//! enter at time 1, and only reach changes at time 1 leave.

use crate::_2_store::Edge;
use differential_dataflow::input::Input;
use differential_dataflow::operators::*;
use differential_dataflow::VecCollection;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use timely::dataflow::operators::probe::Handle;

/// `reach(a, c) <- edge(a, c)`; `reach(a, c) <- reach(a, b), edge(b, c)`.
fn reach<'scope>(edges: VecCollection<'scope, u64, Edge>) -> VecCollection<'scope, u64, Edge> {
    edges.clone().iterate(|scope, inner| {
        let edges = edges.enter(scope);
        inner
            .map(|(from, via)| (via, from))
            .join_map(edges.clone(), |_via, from, to| (*from, *to))
            .concat(edges)
            .distinct()
    })
}

pub struct Settled {
    pub reach: Vec<(Edge, isize)>,
    pub replay: Duration,
    pub delta: Duration,
}

pub fn settle(stored: Vec<Edge>, changes: Vec<(Edge, isize)>) -> Settled {
    timely::execute_directly(move |worker| {
        let sink = Rc::new(RefCell::new(Vec::new()));
        let probe = Handle::new();
        let mut input = worker.dataflow(|scope| {
            let (input, edges) = scope.new_collection::<Edge, isize>();
            let out = Rc::clone(&sink);
            reach(edges)
                .consolidate()
                .inspect(move |(row, time, diff)| out.borrow_mut().push((*row, *time, *diff)))
                .probe_with(&probe);
            input
        });

        let started = Instant::now();
        for edge in stored {
            input.insert(edge);
        }
        input.advance_to(1);
        input.flush();
        worker.step_while(|| probe.less_than(input.time()));
        let replay = started.elapsed();

        let started = Instant::now();
        for (edge, diff) in changes {
            input.update(edge, diff);
        }
        input.advance_to(2);
        input.flush();
        worker.step_while(|| probe.less_than(input.time()));
        let delta = started.elapsed();

        let reach = sink
            .borrow()
            .iter()
            .filter(|(_, time, _)| *time == 1)
            .map(|(row, _, diff)| (*row, *diff))
            .collect();
        Settled { reach, replay, delta }
    })
}
