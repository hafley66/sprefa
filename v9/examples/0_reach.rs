//! Edges in, reach out. Rows come from a client: `POST /commit` on `edge`.
//! `cargo run --example 0_reach -- <DB> [--port N]`. Each Rust form sits under its lisp form.

use std::process::ExitCode;
use v9::_2_ir::Col::Text;
use v9::_2_ir::*;
use v9::_5_ends::IService;

pub fn program() -> Program {
    Program {
        rels: vec![
            // (rel edge (from text) (to text))
            rel("edge", [("from", Text), ("to", Text)]),
            // (derived reach (from text) (to text))
            derived("reach", [("from", Text), ("to", Text)]),
        ],
        rules: vec![
            // (rule (reach from to) (edge from to))
            rule(
                atom("reach", [v("from"), v("to")]),
                [pos("edge", [v("from"), v("to")])],
            ),
            // (rule (reach from to) (reach from middle) (edge middle to))
            rule(
                atom("reach", [v("from"), v("to")]),
                [
                    pos("reach", [v("from"), v("middle")]),
                    pos("edge", [v("middle"), v("to")]),
                ],
            ),
        ],
    }
}

pub fn services() -> Vec<Box<dyn IService>> {
    Vec::new()
}

fn main() -> ExitCode {
    v9::_8_runtime::_5_run::main("0_reach", program, services)
}
