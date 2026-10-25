//! Hop counts from `start` along `step`, skipping `blocked` nodes, capped by `lt`; nodes never
//! reached (negation of a derived rel, a later stratum); distinct start pairs by `neq`.
//! `cargo run --example 1_negation_add -- <DB> [--port N]`. Each Rust form sits under its lisp form.

use std::process::ExitCode;
use v9::_2_ir::Builtin::{Add, Lt, Neq};
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_5_ends::IService;

pub fn program() -> Program {
    Program {
        rels: vec![
            // (rel step (from text) (to text))
            rel("step", [("from", Text), ("to", Text)]),
            // (rel blocked (node text))
            rel("blocked", [("node", Text)]),
            // (rel start (node text))
            rel("start", [("node", Text)]),
            // (derived node (node text))
            derived("node", [("node", Text)]),
            // (derived hops (node text) (count int))
            derived("hops", [("node", Text), ("count", Int)]),
            // (derived reached (node text))
            derived("reached", [("node", Text)]),
            // (derived unreached (node text))
            derived("unreached", [("node", Text)]),
            // (derived rival (left text) (right text))
            derived("rival", [("left", Text), ("right", Text)]),
        ],
        rules: vec![
            // (rule (node from) (step from to))
            rule(
                atom("node", [v("from")]),
                [pos("step", [v("from"), v("to")])],
            ),
            // (rule (node to) (step from to))
            rule(atom("node", [v("to")]), [pos("step", [v("from"), v("to")])]),
            // (rule (hops node 0) (start node))
            rule(
                atom("hops", [v("node"), int(0)]),
                [pos("start", [v("node")])],
            ),
            // (rule (hops to next)
            //   (hops from count) (step from to) (not (blocked to)) (add count 1 next) (lt next 4))
            rule(
                atom("hops", [v("to"), v("next")]),
                [
                    pos("hops", [v("from"), v("count")]),
                    pos("step", [v("from"), v("to")]),
                    neg("blocked", [v("to")]),
                    call(Add, [v("count"), int(1), v("next")]),
                    call(Lt, [v("next"), int(4)]),
                ],
            ),
            // (rule (reached node) (hops node count))
            rule(
                atom("reached", [v("node")]),
                [pos("hops", [v("node"), v("count")])],
            ),
            // (rule (unreached node) (node node) (not (reached node)))
            rule(
                atom("unreached", [v("node")]),
                [pos("node", [v("node")]), neg("reached", [v("node")])],
            ),
            // (rule (rival left right) (start left) (start right) (neq left right))
            rule(
                atom("rival", [v("left"), v("right")]),
                [
                    pos("start", [v("left")]),
                    pos("start", [v("right")]),
                    call(Neq, [v("left"), v("right")]),
                ],
            ),
        ],
    }
}

pub fn services() -> Vec<Box<dyn IService>> {
    Vec::new()
}

fn main() -> ExitCode {
    v9::_8_runtime::_5_run::main("1_negation_add", program, services)
}
