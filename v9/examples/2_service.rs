//! A Request/Answer pair served by an in-file `IService`: each committed `word` asks `len` for its
//! byte length; `long` keeps the words longer than four bytes.
//! `cargo run --example 2_service -- <DB> [--port N]`. Each Rust form sits under its lisp form.

use std::collections::{BTreeMap, BTreeSet};
use std::process::ExitCode;
use v9::_0_id::Dict;
use v9::_2_ir::Builtin::Lt;
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_3_plan::Row;
use v9::_5_ends::{Cadence, Change, IService};

pub fn program() -> Program {
    Program {
        rels: vec![
            // (rel word (word text))
            rel("word", [("word", Text)]),
            // (request len ask_len (word text))
            request("len", "ask_len", [("word", Text)]),
            // (answer len told_len (word text) (len int))
            answer("len", "told_len", [("word", Text), ("len", Int)]),
            // (derived long (word text) (len int))
            derived("long", [("word", Text), ("len", Int)]),
        ],
        rules: vec![
            // (rule (ask_len word) (word word))
            rule(atom("ask_len", [v("word")]), [pos("word", [v("word")])]),
            // (rule (long word len) (told_len word len) (lt 4 len))
            rule(
                atom("long", [v("word"), v("len")]),
                [
                    pos("told_len", [v("word"), v("len")]),
                    call(Lt, [int(4), v("len")]),
                ],
            ),
        ],
    }
}

/// Once: an asked word is answered when asked, then again only on `Core::refresh`. `answered`
/// holds the last answer rows per request row, so a re-ask answers the difference.
#[derive(Default)]
pub struct Len {
    pub ready: Vec<Change>,
    pub answered: BTreeMap<Row, BTreeSet<Row>>,
}

impl IService for Len {
    fn service(&self) -> &str {
        "len"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    /// A retracted request needs nothing: the runtime hides its answers.
    fn call(&mut self, requests: &[Change], dict: &Dict) {
        for request in requests.iter().filter(|request| request.diff > 0) {
            let word = request.row[0];
            let rows = BTreeSet::from([vec![word, dict.bytes[&word].len() as u64]]);
            let before = self.answered.remove(&request.row).unwrap_or_default();
            for (from, minus, diff) in [(&rows, &before, 1), (&before, &rows, -1)] {
                self.ready.extend(from.difference(minus).map(|row| Change {
                    rel: 0,
                    row: row.clone(),
                    diff,
                }));
            }
            self.answered.insert(request.row.clone(), rows);
        }
    }

    fn poll(&mut self, _dict: &mut Dict, out: &mut Vec<Change>) {
        out.append(&mut self.ready);
    }

    fn seed(&mut self, answers: &[Change]) {
        self.ready.clear();
        self.answered.clear();
        for answer in answers {
            self.answered
                .entry(answer.row[..1].to_vec())
                .or_default()
                .insert(answer.row.clone());
        }
    }
}

pub fn services() -> Vec<Box<dyn IService>> {
    vec![Box::new(Len::default())]
}

fn main() -> ExitCode {
    v9::_8_runtime::_5_run::main("2_service", program, services)
}
