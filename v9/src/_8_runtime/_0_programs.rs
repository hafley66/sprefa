//! Built-in programs by name. Programs are Rust until a text front exists; `v9 serve` picks one.

use crate::_2_ir::Col::Text;
use crate::_2_ir::*;

pub type Entry = (&'static str, fn() -> Program);

pub const PROGRAMS: &[Entry] = &[("reach", reach)];

pub fn program(name: &str) -> Option<Program> {
    PROGRAMS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, make)| make())
}

/// A committed `root` asks ryi for its call edges; `reach` is their transitive closure.
pub fn reach() -> Program {
    Program {
        rels: vec![
            rel("root", [("root", Text)]),
            request("ryi", "ryi_ask", [("root", Text)]),
            answer(
                "ryi",
                "ryi_edge",
                [("root", Text), ("caller", Text), ("callee", Text)],
            ),
            derived("edge", [("caller", Text), ("callee", Text)]),
            derived("reach", [("caller", Text), ("callee", Text)]),
        ],
        rules: vec![
            rule(atom("ryi_ask", [v("root")]), [pos("root", [v("root")])]),
            rule(
                atom("edge", [v("caller"), v("callee")]),
                [pos("ryi_edge", [v("root"), v("caller"), v("callee")])],
            ),
            rule(
                atom("reach", [v("caller"), v("callee")]),
                [pos("edge", [v("caller"), v("callee")])],
            ),
            rule(
                atom("reach", [v("caller"), v("callee")]),
                [
                    pos("reach", [v("caller"), v("middle")]),
                    pos("edge", [v("middle"), v("callee")]),
                ],
            ),
        ],
    }
}
