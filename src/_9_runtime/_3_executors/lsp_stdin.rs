//! `(lsp.stdin 1)`: a Continuing reader over the process stdin. tiny-serve
//! writes one JSON line per signed row,
//! `{"rel":"lsp.buffer","sign":1,"args":[...]}`, and the same line with sign
//! -1 to retract. Every line whose relation is `lsp.buffer` lands as a
//! `SignedRow` on the buffer product with weight = sign; other relations are
//! warned and skipped. A goal on `(lsp.stdin 1)` arms the reader; stdin EOF
//! disarms it, so the live loop settles instead of waiting.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::ir_eval::SignedRow;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IService};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

pub const RELATION: &str = "lsp.stdin";
pub const BUFFER: &str = "lsp.buffer";

/// One parsed line: plain data only, terms are minted on the tick thread.
struct Line {
    /// 1 adds, -1 retracts, 0 marks stdin EOF.
    sign: i64,
    args: Vec<Cell>,
}

/// One argument of a buffer line: the only kinds `buffer` declares.
enum Cell {
    Str(String),
    Int(i64),
}

pub struct LspStdin {
    relation: TermId,
    buffer: TermId,
    /// Parsed lines the reader task has handed over, in arrival order.
    lines: Receiver<Line>,
    feed: Option<Sender<Line>>,
    /// The reader task is running.
    armed: bool,
    /// The reader task saw stdin EOF; the clock is over.
    eof: bool,
}

impl LspStdin {
    pub fn new(relation: TermId, buffer: TermId) -> LspStdin {
        let (feed, lines) = channel();
        LspStdin {
            relation,
            buffer,
            lines,
            feed: Some(feed),
            armed: false,
            eof: false,
        }
    }

    fn arm(&mut self) {
        if self.armed {
            return;
        }
        self.armed = true;
        let feed = self.feed.take().expect("a fresh reader owns the channel");
        super::handle().spawn(async move {
            let mut lines = BufReader::new(tokio::io::stdin()).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if feed.send(parse_line(&line)).is_err() {
                            return;
                        }
                        super::wake();
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(target: "dl8::lsp_stdin", %error, "stdin read failed");
                        break;
                    }
                }
            }
            let _ = feed.send(Line {
                sign: 0,
                args: vec![],
            });
            super::wake();
        });
        tracing::info!(target: "dl8::lsp_stdin", "armed");
    }
}

/// One stdin line as plain data. A line that does not parse, names another
/// relation, or does not match the buffer columns is warned and skipped, so
/// one bad line never stops the reader.
fn parse_line(line: &str) -> Line {
    let skip = |reason: &str| {
        tracing::warn!(target: "dl8::lsp_stdin", line = %line, reason, "skipping stdin line");
        Line {
            sign: 0,
            args: vec![],
        }
    };
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return skip("not json"),
    };
    let Some(rel) = value.get("rel").and_then(|value| value.as_str()) else {
        return skip("missing rel");
    };
    if rel != BUFFER {
        return skip("unknown rel");
    }
    let Some(sign) = value.get("sign").and_then(|value| value.as_i64()) else {
        return skip("missing sign");
    };
    if sign != 1 && sign != -1 {
        return skip("sign must be 1 or -1");
    }
    let Some(args) = value.get("args").and_then(|value| value.as_array()) else {
        return skip("missing args");
    };
    let mut cells = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(text) = arg.as_str() {
            cells.push(Cell::Str(text.to_string()));
        } else if let Some(int) = arg.as_i64() {
            cells.push(Cell::Int(int));
        } else {
            return skip("arg is neither string nor int");
        }
    }
    if cells.len() != 4 {
        return skip("buffer takes four columns");
    }
    Line { sign, args: cells }
}

impl IService for LspStdin {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Continuing
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        if self.armed {
            return;
        }
        for &application in pending {
            let on = application_values(u, application)
                .and_then(|values| values.first().copied().flatten())
                .and_then(|value| match u.get(value) {
                    Term::Int(on) => Some(*on),
                    _ => None,
                });
            if on == Some(1) {
                self.arm();
                return;
            }
        }
    }

    fn poll(&mut self, _u: &mut Universe) -> Vec<Row> {
        Vec::new()
    }

    fn armed(&self) -> bool {
        self.armed && !self.eof
    }

    fn poll_signed(&mut self, u: &mut Universe, _timeout: Duration) -> Vec<SignedRow> {
        let mut changes = Vec::new();
        for line in self.lines.try_iter() {
            if line.args.is_empty() {
                self.eof = true;
                tracing::info!(target: "dl8::lsp_stdin", "stdin eof");
                continue;
            }
            let mut args = Vec::with_capacity(line.args.len());
            for cell in line.args {
                let term = match cell {
                    Cell::Str(text) => u.string(&text),
                    Cell::Int(int) => u.int(int),
                };
                args.push(u.compound("const", vec![term]));
            }
            changes.push(SignedRow {
                row: Row {
                    rel: self.buffer,
                    args,
                },
                weight: line.sign,
            });
        }
        changes
    }
}
