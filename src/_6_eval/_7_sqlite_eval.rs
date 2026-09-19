//! `IEvaluate` over rusqlite: one `:memory:` connection, one sqlite_ivm view
//! per program (F3b), every statement inside `sql()`. Rust moves rows in and
//! out; joins, filters and the fixpoint are SQL.

use super::program::{Diagnostic, Program, Row};
use super::term::{Term, TermId, Universe};
use crate::_5_reify::sqlite::{program_plan_with, KernelReach};
use super::kernel::Kernel;
use crate::_5_reify::Stop;
use crate::_6_eval::stratify::stratify;
use crate::_9_runtime::sqlite::{open, sql};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One statement carries at most this many bound values; the bundled build
/// reports `SQLITE_LIMIT_VARIABLE_NUMBER` as 32766.
const BIND_BUDGET: usize = 32766;

/// `term.kind` as `_9_runtime/_1_sqlite.rs` writes it.
const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
const KIND_BOOL: i64 = 2;
const KIND_ATOM: i64 = 3;
const KIND_STR: i64 = 4;
const KIND_COMPOUND: i64 = 5;

const VIEW: &str = "\"program\"";

/// Source tables plus one sqlite_ivm view for the program.
pub trait IEvaluate {
    fn declare(&mut self, program: &Program) -> Result<Declared, Stop>;
    /// Seeds in, one INSERT or DELETE per product per chunk.
    fn apply(&mut self, delta: SeedDelta) -> Result<Applied, Stop>;
    /// Rows of the named products; an empty slice reads every product.
    fn read(&self, products: &[TermId]) -> Result<super::evaluate::Closure, Stop>;
}

pub struct Declared {
    pub view: String,
}

pub struct Applied {
    pub rows: usize,
}

pub struct SeedDelta {
    pub insert: Vec<Row>,
    pub delete: Vec<Row>,
}

pub struct SqliteEvaluate {
    connection: Connection,
    arena: Arc<Mutex<Universe>>,
    diagnostics: Vec<Diagnostic>,
    view_width: usize,
    /// `(relation, arity)` per derived product, for read truncation.
    derived: Vec<(TermId, usize)>,
    /// `(table, relation, arity)` per seeded product.
    seeded: Vec<(String, TermId, usize)>,
    /// A stratify diagnostic stopped the run; reads return no rows.
    halted: bool,
    /// The recursive CTE names the eval lowering caps, in the ordinal order
    /// the view's marker rows carry, plus the marker product tag itself.
    cap_marker: Option<TermId>,
    cap_names: Vec<String>,
}

impl SqliteEvaluate {
    /// One `:memory:` connection: contract pragmas, the sqlite_ivm extension,
    /// every `dl_*` function bound to the arena.
    pub fn connect(arena: Arc<Mutex<Universe>>) -> Result<Self, Stop> {
        let connection = open(Path::new(":memory:")).map_err(|_| Stop::Fail("eval open"))?;
        super::functions::register(&connection, arena.clone())
            .map_err(|_| Stop::Fail("eval functions"))?;
        Ok(SqliteEvaluate {
            connection,
            arena,
            diagnostics: Vec::new(),
            view_width: 0,
            derived: Vec::new(),
            seeded: Vec::new(),
            halted: false,
            cap_marker: None,
            cap_names: Vec::new(),
        })
    }

    /// The store table one seeded product loads into.
    fn table_of(&self, rel: TermId, arity: usize) -> Option<&str> {
        self.seeded
            .iter()
            .find(|(_, found, found_arity)| *found == rel && *found_arity == arity)
            .map(|(table, _, _)| table.as_str())
    }
}

impl IEvaluate for SqliteEvaluate {
    fn declare(&mut self, program: &Program) -> Result<Declared, Stop> {
        let (diagnostics, ddl, view, width, cap_names, marker) = {
            let mut guard = self.arena.lock().map_err(|_| Stop::Fail("eval arena"))?;
            let (_, stratify_diagnostics) = stratify(&mut guard, program);
            if !stratify_diagnostics.is_empty() {
                self.diagnostics = stratify_diagnostics;
                self.halted = true;
                return Ok(Declared {
                    view: "program".to_string(),
                });
            }
            let mut rels: Vec<TermId> = program
                .rules
                .iter()
                .map(|rule| rule.rel)
                .chain(program.rules.iter().flat_map(|rule| rule.body.iter().map(|goal| goal.rel)))
                .chain(program.seeds.iter().map(|row| row.rel))
                .collect();
            rels.sort_by_key(|term| term.0);
            rels.dedup();
            let mut pairs: Vec<(String, TermId)> = rels
                .iter()
                .filter_map(|rel| {
                    let inner = guard.unary(*rel, "ref")?;
                    let name = guard.functor_or_atom(inner).map(|(name, _)| name)?;
                    Some((name.to_string(), *rel))
                })
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
            let mut names: HashMap<String, TermId> = HashMap::new();
            for (name, rel) in pairs {
                names.entry(name).or_insert(rel);
            }
            let none = guard.atom("none");
            let pad = guard.compound("const", vec![none]);
            let marker = guard.atom("recursion_depth_exceeded");
            let (eval_failures, view, width, derived, seeded, cap_names, ddl) = {
                let plan = program_plan_with(
                    &mut guard,
                    program,
                    &names,
                    "main",
                    |u, rel| Kernel::of(u, rel).is_some(),
                    KernelReach::Eval,
                );
                tracing::info!(target: "dl8::eval", nonlinear_sites = plan.nonlinear_sites());
                let eval_failures = plan.eval_failures();
                let cap_names = plan.cap_specs();
                let caps: Vec<(i64, i64, String)> = cap_names
                    .iter()
                    .enumerate()
                    .map(|(ordinal, name)| (marker.0 as i64, ordinal as i64, name.clone()))
                    .collect();
                let (view, width) = match plan.view(pad, &caps) {
                    Some(pair) => pair,
                    None => (String::new(), 0),
                };
                let derived = plan.derived_tags();
                let mut seed_keys: Vec<(u32, usize)> = program
                    .seeds
                    .iter()
                    .map(|row| (row.rel.0, row.args.len()))
                    .collect();
                seed_keys.sort();
                seed_keys.dedup();
                let seeded: Vec<(String, TermId, usize)> = seed_keys
                    .iter()
                    .map(|&(rel, arity)| (plan.table_name((TermId(rel), arity)), TermId(rel), arity))
                    .collect();
                let mut ddl = dictionary_ddl();
                for (table, _, arity) in &seeded {
                    ddl.push('\n');
                    ddl.push_str(&product_ddl(table, *arity));
                }
                (eval_failures, view, width, derived, seeded, cap_names, ddl)
            };
            self.derived = derived;
            self.seeded = seeded;
            let mut diagnostics: Vec<Diagnostic> = eval_failures
                .iter()
                .map(|(shape, count)| {
                    if shape == "malformed_aggregate" {
                        let n = guard.int(*count as i64);
                        Diagnostic {
                            phase: "evaluate",
                            payload: guard.compound("malformed_aggregate_head", vec![n]),
                        }
                    } else {
                        let shape = guard.atom(shape);
                        Diagnostic {
                            phase: "emit",
                            payload: guard.compound("not_built_yet", vec![shape]),
                        }
                    }
                })
                .collect();
            diagnostics.sort_by(|a, b| (a.phase, a.payload.0).cmp(&(b.phase, b.payload.0)));
            diagnostics.dedup();
            (diagnostics, ddl, view, width, cap_names, marker)
        };
        // Below this point no arena guard is held: every statement runs the
        // `dl_*` functions, and each function locks the same mutex.
        self.diagnostics = diagnostics;
        self.cap_marker = Some(marker);
        self.cap_names = cap_names;
        sql(&self.connection, "declare_tables", |connection| {
            connection.execute_batch(&ddl).map(|()| ((), 0))
        })
        .map_err(|e| {
            tracing::error!(target: "dl8::eval", phase = "declare_tables", error = %e);
            Stop::Fail("eval declare_tables")
        })?;
        let flush = {
            let guard = self.arena.lock().map_err(|_| Stop::Fail("eval arena"))?;
            flush_rows(&guard)
        };
        flush_commit(&self.connection, flush).map_err(|_| Stop::Fail("eval flush"))?;
        self.write_delta(&program.seeds, &[])?;
        if !view.is_empty() {
            tracing::debug!(target: "dl8::eval", view = %view, "declare_view sql");
            sql(&self.connection, "declare_view", |connection| {
                connection.execute_batch(&view).map(|()| ((), 0))
            })
            .map_err(|e| {
                tracing::error!(target: "dl8::eval", phase = "declare_view", error = %e);
                Stop::Fail("eval declare_view")
            })?;
        }
        self.view_width = width;
        Ok(Declared {
            view: "program".to_string(),
        })
    }

    fn apply(&mut self, delta: SeedDelta) -> Result<Applied, Stop> {
        let rows = self.write_delta(&delta.insert, &delta.delete)?;
        Ok(Applied { rows })
    }

    fn read(&self, products: &[TermId]) -> Result<super::evaluate::Closure, Stop> {
        if self.halted {
            return Ok(super::evaluate::Closure {
                rows: Vec::new(),
                diagnostics: self.diagnostics.clone(),
            });
        }
        let mut rows: Vec<Row> = Vec::new();
        let width = self.view_width;
        if width > 0 {
            let columns: Vec<String> = (0..width)
                .map(|position| format!("\"c{position}\""))
                .collect();
            let select = format!("SELECT \"product\", {} FROM {VIEW}", columns.join(", "));
            sql(&self.connection, "read_view", |connection| {
                let mut statement = connection.prepare(&select)?;
                let mut cursor = statement.query([])?;
                while let Some(row) = cursor.next()? {
                    let rel = TermId(row.get::<_, i64>(0)? as u32);
                    let mut args: Vec<TermId> = Vec::with_capacity(width);
                    for at in 1..=width {
                        args.push(TermId(row.get::<_, i64>(at)? as u32));
                    }
                    rows.push(Row { rel, args });
                }
                Ok(((), rows.len()))
            })
            .map_err(|_| Stop::Fail("eval read"))?;
        }
        for (table, rel, arity) in &self.seeded {
            if !products.is_empty() && !products.contains(rel) {
                continue;
            }
            let columns: Vec<String> = (0..*arity)
                .map(|position| format!("\"c{position}_term\""))
                .collect();
            let select = format!("SELECT {} FROM {table}", columns.join(", "));
            sql(&self.connection, "read_seeds", |connection| {
                let mut statement = connection.prepare(&select)?;
                let mut cursor = statement.query([])?;
                while let Some(row) = cursor.next()? {
                    let mut args: Vec<TermId> = Vec::with_capacity(*arity);
                    for at in 0..*arity {
                        args.push(TermId(row.get::<_, i64>(at)? as u32));
                    }
                    rows.push(Row { rel: *rel, args });
                }
                Ok(((), rows.len()))
            })
            .map_err(|_| Stop::Fail("eval read seeds"))?;
        }
        // The guard comes down after the reads: the view select runs the
        // `dl_*` functions, and each one locks the same arena.
        let mut guard = self.arena.lock().map_err(|_| Stop::Fail("eval arena"))?;
        let mut diagnostics = self.diagnostics.clone();
        let mut kept: Vec<Row> = Vec::with_capacity(rows.len());
        for row in rows {
            if self.cap_marker == Some(row.rel) {
                let ordinal = row
                    .args
                    .first()
                    .map(|term| term.0 as usize)
                    .unwrap_or(usize::MAX);
                let payload = match self.cap_names.get(ordinal) {
                    Some(name) => {
                        let name = guard.atom(name);
                        guard.compound("recursion_depth_exceeded", vec![name])
                    }
                    None => guard.atom("recursion_depth_exceeded"),
                };
                diagnostics.push(Diagnostic {
                    phase: "evaluate",
                    payload,
                });
            } else {
                kept.push(row);
            }
        }
        for row in &mut kept {
            let arity = arity_of(&self.derived, &self.seeded, row.rel);
            row.args.truncate(arity);
        }
        kept.retain(|row| products.is_empty() || products.contains(&row.rel));
        kept.sort_by(|a, b| {
            guard
                .cmp(a.rel, b.rel)
                .then_with(|| guard.cmp_rows(&a.args, &b.args))
        });
        kept.dedup();
        diagnostics.sort_by(|a, b| (a.phase, a.payload.0).cmp(&(b.phase, b.payload.0)));
        diagnostics.dedup();
        Ok(super::evaluate::Closure {
            rows: kept,
            diagnostics,
        })
    }
}

fn arity_of(derived: &[(TermId, usize)], seeded: &[(String, TermId, usize)], rel: TermId) -> usize {
    derived
        .iter()
        .copied()
        .chain(seeded.iter().map(|(_, rel, arity)| (*rel, *arity)))
        .find(|(found, _)| *found == rel)
        .map(|(_, arity)| arity)
        .unwrap_or(0)
}

/// The dictionary: `sym`, `term`, `term_arg`, every cell a `TermId`; plus the
/// one-row `unit` table a bodyless rule reads instead of a FROM-less query.
fn dictionary_ddl() -> String {
    "\
     CREATE TABLE \"main.sym\" (\
       \"id\" INTEGER PRIMARY KEY,\
       \"text\" TEXT NOT NULL UNIQUE);\
     CREATE TABLE \"main.term\" (\
       \"id\" INTEGER PRIMARY KEY,\
       \"kind\" INTEGER NOT NULL,\
       \"ival\" INTEGER NOT NULL,\
       \"rval\" REAL NOT NULL,\
       \"sym\" INTEGER NOT NULL REFERENCES \"main.sym\"(\"id\"));\
     CREATE TABLE \"main.term_arg\" (\
       \"term\" INTEGER NOT NULL REFERENCES \"main.term\"(\"id\"),\
       \"position\" INTEGER NOT NULL,\
       \"child\" INTEGER NOT NULL REFERENCES \"main.term\"(\"id\"),\
       PRIMARY KEY (\"term\", \"position\"),\
       CHECK (\"child\" < \"term\")) WITHOUT ROWID;\
     CREATE TABLE \"main.unit\" (\"one\" INTEGER NOT NULL);\
     INSERT OR IGNORE INTO \"main.unit\" VALUES (1);"
        .to_string()
}

/// One `UNIQUE` over every column, so a product row is a set element.
fn product_ddl(table: &str, arity: usize) -> String {
    let columns: Vec<String> = (0..arity)
        .map(|position| format!("\"c{position}_term\" INTEGER NOT NULL"))
        .collect();
    let unique: Vec<String> = (0..arity)
        .map(|position| format!("\"c{position}_term\""))
        .collect();
    format!(
        "CREATE TABLE IF NOT EXISTS {table} ({}, UNIQUE ({}));",
        columns.join(", "),
        unique.join(", ")
    )
}

impl SqliteEvaluate {
    /// One multi-row `INSERT OR IGNORE` per product per chunk of the bind
    /// budget; deletes run one row-value `DELETE ... IN (VALUES ...)` per chunk.
    fn write_delta(&self, insert: &[Row], delete: &[Row]) -> Result<usize, Stop> {
        let mut written = 0;
        let mut grouped: HashMap<(TermId, usize), Vec<Vec<i64>>> = HashMap::new();
        for row in insert {
            grouped
                .entry((row.rel, row.args.len()))
                .or_default()
                .push(row.args.iter().map(|term| term.0 as i64).collect());
        }
        for ((rel, arity), values) in &grouped {
            let Some(table) = self.table_of(*rel, *arity) else {
                continue;
            };
            let columns: Vec<String> = (0..*arity)
                .map(|position| format!("\"c{position}_term\""))
                .collect();
            let chunk_rows = (BIND_BUDGET / (*arity).max(1)).max(1);
            for chunk in values.chunks(chunk_rows) {
                let placeholders = values_placeholder(*arity, chunk.len());
                let statement = format!(
                    "INSERT OR IGNORE INTO {table} ({}) VALUES {placeholders}",
                    columns.join(", ")
                );
                let bound: Vec<SqlValue> = chunk
                    .iter()
                    .flatten()
                    .map(|value| SqlValue::Integer(*value))
                    .collect();
                let count = chunk.len();
                sql(&self.connection, "insert_seeds", |connection| {
                    let changes = connection.execute(&statement, rusqlite::params_from_iter(bound))?;
                    Ok(((), changes))
                })
                .map_err(|_| Stop::Fail("eval insert_seeds"))?;
                written += count;
            }
        }
        let mut removals: HashMap<(TermId, usize), Vec<Vec<i64>>> = HashMap::new();
        for row in delete {
            removals
                .entry((row.rel, row.args.len()))
                .or_default()
                .push(row.args.iter().map(|term| term.0 as i64).collect());
        }
        for ((rel, arity), values) in &removals {
            let Some(table) = self.table_of(*rel, *arity) else {
                continue;
            };
            let columns: Vec<String> = (0..*arity)
                .map(|position| format!("\"c{position}_term\""))
                .collect();
            let chunk_rows = (BIND_BUDGET / (2 * (*arity).max(1))).max(1);
            for chunk in values.chunks(chunk_rows) {
                let placeholders = values_placeholder(*arity, chunk.len());
                let statement = format!(
                    "DELETE FROM {table} WHERE ({}) IN (VALUES {placeholders})",
                    columns.join(", ")
                );
                let bound: Vec<SqlValue> = chunk
                    .iter()
                    .flatten()
                    .map(|value| SqlValue::Integer(*value))
                    .collect();
                sql(&self.connection, "delete_seeds", |connection| {
                    let changes = connection.execute(&statement, rusqlite::params_from_iter(bound))?;
                    Ok(((), changes))
                })
                .map_err(|_| Stop::Fail("eval delete_seeds"))?;
            }
        }
        Ok(written)
    }
}

fn values_placeholder(arity: usize, rows: usize) -> String {
    let row = format!(
        "({})",
        std::iter::repeat("?")
            .take(arity)
            .collect::<Vec<_>>()
            .join(", ")
    );
    std::iter::repeat(row)
        .take(rows)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The arena rows the dictionary commit writes. Collected under the arena
/// lock, committed without it: no guard crosses a `sql()` call, and the
/// connection carries the `dl_*` functions that re-lock the same arena.
struct Flush {
    syms: Vec<(i64, String)>,
    terms: Vec<(i64, i64, i64, f64, i64)>,
    arguments: Vec<(i64, i64, i64)>,
}

/// The whole arena mirrors into the dictionary; an id equals its index.
fn flush_rows(u: &Universe) -> Flush {
    let syms = u
        .syms
        .iter()
        .enumerate()
        .map(|(id, text)| (id as i64, text.clone()))
        .collect();
    let mut terms: Vec<(i64, i64, i64, f64, i64)> = Vec::new();
    let mut arguments: Vec<(i64, i64, i64)> = Vec::new();
    for (id, term) in u.terms.iter().enumerate() {
        let id = id as i64;
        match term {
            Term::Int(n) => terms.push((id, KIND_INT, *n, 0.0, 0)),
            Term::Float(x) => terms.push((id, KIND_FLOAT, 0, x.0, 0)),
            Term::Bool(b) => terms.push((id, KIND_BOOL, i64::from(*b), 0.0, 0)),
            Term::Atom(s) => terms.push((id, KIND_ATOM, 0, 0.0, s.0 as i64)),
            Term::Str(s) => terms.push((id, KIND_STR, 0, 0.0, s.0 as i64)),
            Term::Compound(s, args) => {
                terms.push((id, KIND_COMPOUND, 0, 0.0, s.0 as i64));
                for (position, child) in args.iter().enumerate() {
                    arguments.push((id, position as i64, child.0 as i64));
                }
            }
        }
    }
    Flush {
        syms,
        terms,
        arguments,
    }
}

fn flush_commit(connection: &Connection, data: Flush) -> rusqlite::Result<()> {
    let Flush {
        syms,
        terms,
        arguments,
    } = data;
    sql(connection, "flush_syms", |connection| {
        let chunk_rows = (BIND_BUDGET / 2).max(1);
        let ids: Vec<SqlValue> = syms
            .iter()
            .flat_map(|(id, text)| [SqlValue::Integer(*id), SqlValue::Text(text.into())])
            .collect();
        for chunk in ids.chunks(2 * chunk_rows) {
            let placeholders = values_placeholder(2, chunk.len() / 2);
            let insert = format!(
                "INSERT OR IGNORE INTO \"main.sym\" (\"id\", \"text\") VALUES {placeholders}"
            );
            connection.execute(&insert, rusqlite::params_from_iter(chunk))?;
        }
        Ok(((), syms.len()))
    })?;
    sql(connection, "flush_terms", |connection| {
        let chunk_rows = (BIND_BUDGET / 5).max(1);
        for chunk in terms.chunks(chunk_rows) {
            let placeholders = values_placeholder(5, chunk.len());
            let insert = format!("INSERT OR IGNORE INTO \"main.term\" VALUES {placeholders}");
            let bound: Vec<SqlValue> = chunk
                .iter()
                .flat_map(|(id, kind, ival, rval, sym)| {
                    [
                        SqlValue::Integer(*id),
                        SqlValue::Integer(*kind),
                        SqlValue::Integer(*ival),
                        SqlValue::Real(*rval),
                        SqlValue::Integer(*sym),
                    ]
                })
                .collect();
            connection.execute(&insert, rusqlite::params_from_iter(bound))?;
        }
        Ok(((), terms.len()))
    })?;
    sql(connection, "flush_term_args", |connection| {
        let chunk_rows = (BIND_BUDGET / 3).max(1);
        for chunk in arguments.chunks(chunk_rows) {
            let placeholders = values_placeholder(3, chunk.len());
            let insert = format!("INSERT OR IGNORE INTO \"main.term_arg\" VALUES {placeholders}");
            let bound: Vec<SqlValue> = chunk
                .iter()
                .flat_map(|(term, position, child)| {
                    [
                        SqlValue::Integer(*term),
                        SqlValue::Integer(*position),
                        SqlValue::Integer(*child),
                    ]
                })
                .collect();
            connection.execute(&insert, rusqlite::params_from_iter(bound))?;
        }
        Ok(((), arguments.len()))
    })?;
    Ok(())
}

/// `evaluate` over sqlite: the universe moves into the shared arena for the
/// run and moves back after the connection drops.
/// The kernel `nil` bootstrap the Rust engine plants before fixing: one row
/// holding the empty list.
fn nil_seed(u: &mut Universe) -> Row {
    let rel = {
        let n = u.atom("nil");
        let k = u.compound("kernel", vec![n]);
        u.compound("ref", vec![k])
    };
    let e = u.empty_list();
    let arg = u.compound("const", vec![e]);
    Row {
        rel,
        args: vec![arg],
    }
}

pub fn evaluate_sqlite(
    u: &mut Universe,
    program: &Program,
    fx: &mut dyn FnMut(super::Trace),
) -> super::evaluate::Closure {
    let mut program = program.clone();
    let nil = nil_seed(u);
    program.seeds.push(nil);
    let arena = Arc::new(Mutex::new(std::mem::take(u)));
    let run = SqliteEvaluate::connect(arena.clone()).and_then(|mut engine| {
        engine.declare(&program)?;
        engine.apply(SeedDelta {
            insert: program.seeds.clone(),
            delete: Vec::new(),
        })?;
        engine.read(&[])
    });
    let closure = match run {
        Ok(closure) => closure,
        Err(Stop::Fail(message)) => {
            let mut guard = arena.lock().expect("eval arena poisoned");
            let message = guard.atom(message);
            let payload = guard.compound("not_built_yet", vec![message]);
            super::evaluate::Closure {
                rows: Vec::new(),
                diagnostics: vec![Diagnostic {
                    phase: "emit",
                    payload,
                }],
            }
        }
    };
    {
        let mut guard = arena.lock().expect("eval arena poisoned");
        *u = std::mem::replace(&mut *guard, Universe::default());
    }
    fx(super::Trace::Closure {
        rows: closure.rows.len(),
    });
    closure
}

#[cfg(test)]
mod probes {
    use super::*;
    use crate::_6_eval::json::program_from_json;

    #[test]
    fn plan_sees_transitive_rules() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/oracle/eval/0_transitive.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut u = Universe::new();
        let program = program_from_json(&mut u, value.get("program").unwrap_or(&value)).unwrap();
        println!("rules={} seeds={}", program.rules.len(), program.seeds.len());
        for rule in &program.rules {
            println!("rule rel={} arity={}", u.display(rule.rel), rule.head.len());
        }
        let mut rels: Vec<TermId> = program
            .rules
            .iter()
            .map(|rule| rule.rel)
            .chain(program.rules.iter().flat_map(|rule| rule.body.iter().map(|goal| goal.rel)))
            .collect();
        rels.sort_by_key(|term| term.0);
        rels.dedup();
        let mut names = HashMap::new();
        for rel in &rels {
            if let Some(inner) = u.unary(*rel, "ref") {
                if let Some((name, _)) = u.functor_or_atom(inner) {
                    names.entry(name.to_string()).or_insert(*rel);
                }
            }
        }
        println!("names={names:?}");
        let pad = { let n = u.atom("none"); u.compound("const", vec![n]) };
        let plan = crate::_5_reify::sqlite::program_plan_with(&mut u, &program, &names, "main", |u, rel| crate::_6_eval::kernel::Kernel::of(u, rel).is_some(), crate::_5_reify::sqlite::KernelReach::Eval);
        println!("failures={:?}", plan.failures());
        println!("derived_tags={:?} view_failures_above", plan.derived_tags());
        let view = plan.view(pad, &[]);
        let (ddl, w) = view.unwrap();
        println!("WIDTH={w}");
        println!("DDL={ddl}");
    }
}
