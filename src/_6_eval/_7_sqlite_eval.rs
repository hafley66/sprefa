//! `IEvaluate` over rusqlite: one `:memory:` connection, one sqlite_ivm view
//! per program (F3b), every statement inside `sql()`. Rust moves rows in and
//! out; joins, filters and the fixpoint are SQL.

use super::program::{Arg, Diagnostic, Goal, Polarity, Program, Row, Rule};
use super::term::{Term, TermId, Universe};
use crate::_5_reify::sqlite::{program_plan_with, KernelReach};
use super::kernel::Kernel;
use crate::_5_reify::Stop;
use crate::_6_eval::stratify::stratify;
use super::evaluate::needs_bound_head;
use crate::_9_runtime::sqlite::{open, sql};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
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

/// Product name of the synthesized `intern` twin rules.
const INTERN_ROWS: &str = "intern_rows";
/// Product names of the served-goal twin rules: `effect/2` and
/// `intern_snapshot/3` rows, one per evaluation of a served goal.
const EFFECT_ROWS: &str = "effect_rows";
const SNAPSHOT_ROWS: &str = "snapshot_rows";

/// Rounds a nonlinear component may take before its rows stop changing. It
/// protects against a step that mints a new term every round (the same
/// runaway the recursive CTE's depth cap catches); one round is one pass of
/// every INSERT OR IGNORE statement of every materialized component.
const NONLINEAR_ROUND_LIMIT: usize = 64;

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
    /// Demand and adorned products: joined by the view, never read out.
    hidden: HashSet<TermId>,
    /// INSERT OR IGNORE statements of the nonlinear components (F4b).
    rounds: Vec<String>,
    /// The plain tables those statements fill, emptied before a re-run.
    material_tables: Vec<String>,
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
            hidden: HashSet::new(),
            rounds: Vec::new(),
            material_tables: Vec::new(),
        })
    }

    /// A retracted seed cannot be undone by INSERT OR IGNORE rounds: the
    /// plain tables empty and the rounds run again over the new sources.
    fn reset_rounds(&mut self) -> Result<(), Stop> {
        for table in &self.material_tables {
            let statement = format!("DELETE FROM {table}");
            sql(&self.connection, "reset_material", |connection| {
                let n = connection.execute(&statement, [])?;
                Ok((n, n))
            })
            .map_err(|_| Stop::Fail("eval reset_material"))?;
        }
        self.run_rounds()
    }

    /// F4b: every round statement of every nonlinear component, repeated
    /// until a round changes no row or `NONLINEAR_ROUND_LIMIT` stops it.
    fn run_rounds(&mut self) -> Result<(), Stop> {
        if self.rounds.is_empty() {
            return Ok(());
        }
        for round in 0..NONLINEAR_ROUND_LIMIT {
            let mut changed = 0usize;
            for statement in &self.rounds {
                let rows = sql(&self.connection, "nonlinear_round", |connection| {
                    let n = connection.execute(statement, [])?;
                    Ok((n, n))
                })
                .map_err(|e| {
                    tracing::error!(target: "dl8::eval", phase = "nonlinear_round", error = %e);
                    Stop::Fail("eval nonlinear_round")
                })?;
                changed += rows;
            }
            tracing::info!(target: "dl8::eval", round, changed, "nonlinear round");
            if changed == 0 {
                return Ok(());
            }
        }
        let mut guard = self.arena.lock().map_err(|_| Stop::Fail("eval arena"))?;
        let limit = guard.int(NONLINEAR_ROUND_LIMIT as i64);
        self.diagnostics.push(Diagnostic {
            phase: "evaluate",
            payload: guard.compound("nonlinear_round_limit_exceeded", vec![limit]),
        });
        Ok(())
    }

    /// A table for every product a delta names that has none yet. Every
    /// product a rule reads has a table from `declare`, so such a product
    /// is seed-only: it never touches the view.
    fn cover(&mut self, rows: &[Row]) -> Result<(), Stop> {
        let mut keys: Vec<(TermId, usize)> = rows
            .iter()
            .filter(|row| self.table_of(row.rel, row.args.len()).is_none())
            .map(|row| (row.rel, row.args.len()))
            .collect();
        keys.sort_by_key(|key| (key.0 .0, key.1));
        keys.dedup();
        for (rel, arity) in keys {
            let table = format!("\"main.n{}_a{arity}\"", rel.0);
            let ddl = product_ddl(&table, arity);
            sql(&self.connection, "declare_tables", |connection| {
                connection.execute_batch(&ddl).map(|()| ((), 0))
            })
            .map_err(|_| Stop::Fail("eval declare_tables"))?;
            tracing::debug!(target: "dl8::eval", rel = rel.0, arity, "seed-only product table added");
            self.seeded.push((table, rel, arity));
        }
        Ok(())
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
        let (diagnostics, ddl, view, width, cap_names, marker, rounds, material_tables) = {
            let mut guard = self.arena.lock().map_err(|_| Stop::Fail("eval arena"))?;
            let (_, stratify_diagnostics) = stratify(&mut guard, program);
            if !stratify_diagnostics.is_empty() {
                self.diagnostics = stratify_diagnostics;
                self.halted = true;
                return Ok(Declared {
                    view: "program".to_string(),
                });
            }
            let program = with_effects(&mut guard, program);
            let (program, hidden) = with_demand(&mut guard, &program);
            let program = &with_intern_rows(&mut guard, &program);
            self.hidden = hidden;
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
            let (eval_failures, view, width, derived, seeded, cap_names, ddl, rounds, material_tables) = {
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
                let seeded: Vec<(String, TermId, usize)> = plan
                    .seeded_keys()
                    .into_iter()
                    .map(|(rel, arity)| (plan.table_name((rel, arity)), rel, arity))
                    .collect();
                let mut ddl = dictionary_ddl();
                for (table, _, arity) in &seeded {
                    ddl.push('\n');
                    ddl.push_str(&product_ddl(table, *arity));
                }
                let mut material_tables = Vec::new();
                for (table, columns) in plan.material_tables() {
                    ddl.push('\n');
                    ddl.push_str(&product_ddl(&table, columns.len()));
                    material_tables.push(table);
                }
                let rounds = plan.round_statements();
                (eval_failures, view, width, derived, seeded, cap_names, ddl, rounds, material_tables)
            };
            self.derived = derived;
            self.seeded = seeded;
            // Rust stops on a malformed aggregate head and returns no rows.
            if eval_failures.iter().any(|(shape, _)| shape == "malformed_aggregate") {
                self.halted = true;
            }
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
            (diagnostics, ddl, view, width, cap_names, marker, rounds, material_tables)
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
        self.rounds = rounds;
        self.material_tables = material_tables;
        self.run_rounds()?;
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
                    // Only view rows are padded to the widest arity.
                    if self.cap_marker != Some(rel) {
                        args.truncate(arity_of(&self.derived, &[], rel));
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
        let intern_alias = {
            let name = guard.atom(INTERN_ROWS);
            guard.compound("ref", vec![name])
        };
        let kernel = |guard: &mut Universe, name: &str| {
            let name = guard.atom(name);
            let kernel = guard.compound("kernel", vec![name]);
            guard.compound("ref", vec![kernel])
        };
        let alias = |guard: &mut Universe, name: &str| {
            let name = guard.atom(name);
            guard.compound("ref", vec![name])
        };
        let intern_rel = kernel(&mut guard, "intern");
        let aliases = [
            (intern_alias, intern_rel),
            (alias(&mut guard, EFFECT_ROWS), kernel(&mut guard, "effect")),
            (alias(&mut guard, SNAPSHOT_ROWS), kernel(&mut guard, "intern_snapshot")),
        ];
        for row in &mut kept {
            if let Some((_, real)) = aliases.iter().find(|(twin, _)| *twin == row.rel) {
                row.rel = *real;
            }
        }
        kept.retain(|row| !self.hidden.contains(&row.rel));
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
/// A served goal (a relation the outside answers, with no rule of its own)
/// writes `effect(Relation, Application)` and an `intern_snapshot` row on
/// every evaluation, hit or miss (`Eval::write_effect`). The twin rule's
/// body is the goal's prefix plus the `cons` chain and `intern` call that
/// build the application; an unbound argument is `const(none)`.
fn with_effects(u: &mut Universe, program: &Program) -> Program {
    let ruled: HashSet<TermId> = program.rules.iter().map(|rule| rule.rel).collect();
    let served: Vec<TermId> = program
        .served
        .iter()
        .copied()
        .filter(|rel| !ruled.contains(rel))
        .collect();
    let snapshot_rel = {
        let name = u.atom("intern_snapshot");
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let snapshot_alias = {
        let name = u.atom(SNAPSHOT_ROWS);
        u.compound("ref", vec![name])
    };
    let mut out = program.clone();
    if served.is_empty() {
        return out;
    }
    let effect_rel = {
        let name = u.atom("effect");
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let effect_alias = {
        let name = u.atom(EFFECT_ROWS);
        u.compound("ref", vec![name])
    };
    for rule in &mut out.rules {
        for goal in &mut rule.body {
            if goal.rel == snapshot_rel {
                goal.rel = snapshot_alias;
            } else if goal.rel == effect_rel {
                goal.rel = effect_alias;
            }
        }
    }
    {
        let args: Vec<Arg> = (0..3).map(|i| Arg::Var(super::program::VarId(i))).collect();
        out.rules.push(Rule {
            rel: snapshot_alias,
            head: args.clone(),
            body: vec![Goal { polarity: Polarity::Positive, rel: snapshot_rel, args }],
            vars: (0..3).map(|i| u.atom(&format!("__s{i}"))).collect(),
        });
    }
    let none = {
        let name = u.atom("none");
        u.compound("const", vec![name])
    };
    let kernel = |u: &mut Universe, name: &str| {
        let name = u.atom(name);
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let nil_rel = kernel(u, "nil");
    let cons_rel = kernel(u, "cons");
    let intern_rel = kernel(u, "intern");
    let var = |index: usize| Arg::Var(super::program::VarId(index as u32));
    for rule in &program.rules {
        let mut bound: HashSet<u32> = HashSet::new();
        for (at, goal) in rule.body.iter().enumerate() {
            let positive = goal.polarity == Polarity::Positive;
            if positive && served.contains(&goal.rel) {
                let mut vars = rule.vars.clone();
                let mut fresh = |u: &mut Universe, vars: &mut Vec<TermId>, label: &str| {
                    let index = vars.len();
                    vars.push(u.atom(&format!("__{label}{index}")));
                    var(index)
                };
                let mut body: Vec<Goal> = rule.body[..at].to_vec();
                let cells: Vec<Arg> = goal
                    .args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Var(v) if bound.contains(&v.0) => arg.clone(),
                        Arg::Ground(_) => arg.clone(),
                        _ => Arg::Ground(none),
                    })
                    .collect();
                let mut list = fresh(u, &mut vars, "nil");
                body.push(Goal { polarity: Polarity::Positive, rel: nil_rel, args: vec![list.clone()] });
                for cell in cells.iter().rev() {
                    let next = fresh(u, &mut vars, "cons");
                    body.push(Goal {
                        polarity: Polarity::Positive,
                        rel: cons_rel,
                        args: vec![cell.clone(), list.clone(), next.clone()],
                    });
                    list = next;
                }
                let application = fresh(u, &mut vars, "app");
                body.push(Goal {
                    polarity: Polarity::Positive,
                    rel: intern_rel,
                    args: vec![Arg::Ground(goal.rel), list.clone(), application.clone()],
                });
                out.rules.push(Rule {
                    rel: effect_alias,
                    head: vec![Arg::Ground(goal.rel), application.clone()],
                    body: body.clone(),
                    vars: vars.clone(),
                });
                out.rules.push(Rule {
                    rel: snapshot_alias,
                    head: vec![Arg::Ground(goal.rel), list, application],
                    body,
                    vars,
                });
            }
            if positive {
                for arg in &goal.args {
                    if let Arg::Var(v) = arg {
                        bound.insert(v.0);
                    }
                }
            }
        }
    }
    out
}

/// Magic sets (F5a): a product some rule of which needs bound head arguments
/// is also proved under each caller's binding. A call site with bound
/// positions B feeds a demand product; an adorned copy of every rule joins
/// it; the caller reads the adorned copy. The originals still fire bottom-up.
fn with_demand(u: &mut Universe, program: &Program) -> (Program, HashSet<TermId>) {
    let snapshots = {
        let name = u.atom("intern_snapshot");
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let mut by_rel: HashMap<TermId, Vec<Rule>> = HashMap::new();
    for rule in &program.rules {
        by_rel.entry(rule.rel).or_default().push(rule.clone());
    }
    let demanded: HashSet<TermId> = by_rel
        .iter()
        .filter(|(_, rules)| rules.iter().any(|rule| needs_bound_head(u, rule, snapshots)))
        .map(|(rel, _)| *rel)
        .collect();
    let seeded: HashSet<TermId> = program.seeds.iter().map(|row| row.rel).collect();
    // Pass-through variable names exist from the first run on, so a reload
    // with more seeded products mints nothing new.
    let widest = program.rules.iter().map(|rule| rule.head.len()).max().unwrap_or(0);
    let pass_vars: Vec<TermId> = (0..widest).map(|i| u.atom(&format!("__s{i}"))).collect();
    let mut out = program.clone();
    if demanded.is_empty() {
        return (out, HashSet::new());
    }
    let mut hidden: HashSet<TermId> = HashSet::new();
    let mut adorned: HashMap<(TermId, Vec<usize>), (TermId, TermId)> = HashMap::new();
    let mut queue: Vec<Rule> = std::mem::take(&mut out.rules);
    let mut done: Vec<Rule> = Vec::new();
    // Every adorned copy re-enters the queue, so the walk is bounded by the
    // number of (product, bound positions) pairs times the rule count.
    let budget = program.rules.len() * (1 + program.rules.len() * 8);
    for _ in 0..budget {
        let Some(mut rule) = queue.pop() else {
            break;
        };
        let mut bound: HashSet<u32> = HashSet::new();
        for at in 0..rule.body.len() {
            let goal = rule.body[at].clone();
            let positive = goal.polarity == Polarity::Positive;
            if positive && demanded.contains(&goal.rel) {
                let positions: Vec<usize> = goal
                    .args
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| match arg {
                        Arg::Var(v) => bound.contains(&v.0),
                        Arg::Ground(_) => true,
                        _ => false,
                    })
                    .map(|(position, _)| position)
                    .collect();
                let key = (goal.rel, positions.clone());
                let (demand_rel, adorned_rel) = match adorned.get(&key) {
                    Some(pair) => *pair,
                    None => {
                        let base = u
                            .unary(goal.rel, "ref")
                            .and_then(|inner| u.functor_or_atom(inner).map(|(name, _)| name.to_string()))
                            .unwrap_or_else(|| format!("n{}", goal.rel.0));
                        let mask: String = positions.iter().map(|p| p.to_string()).collect();
                        // The suffix is the rel term itself, so the name survives a
                        // db reload that renumbers the arena.
                        let spelled = u.display(goal.rel).to_string();
                        let demand_name = u.atom(&format!("dmd__{base}__{mask}__{spelled}"));
                        let demand_rel = u.compound("ref", vec![demand_name]);
                        let adorned_name = u.atom(&format!("adn__{base}__{mask}__{spelled}"));
                        let adorned_rel = u.compound("ref", vec![adorned_name]);
                        adorned.insert(key, (demand_rel, adorned_rel));
                        hidden.insert(demand_rel);
                        hidden.insert(adorned_rel);
                        for original in by_rel.get(&goal.rel).into_iter().flatten() {
                            let mut body = vec![Goal {
                                polarity: Polarity::Positive,
                                rel: demand_rel,
                                args: positions.iter().map(|p| original.head[*p].clone()).collect(),
                            }];
                            body.extend(original.body.iter().cloned());
                            queue.push(Rule {
                                rel: adorned_rel,
                                head: original.head.clone(),
                                body,
                                vars: original.vars.clone(),
                            });
                        }
                        if seeded.contains(&goal.rel) {
                            // The pass-through reads the product's own table
                            // and is never itself a demanded call site.
                            let arity = goal.args.len();
                            let vars: Vec<TermId> = pass_vars[..arity.min(pass_vars.len())].to_vec();
                            let args: Vec<Arg> = (0..arity).map(|i| Arg::Var(super::program::VarId(i as u32))).collect();
                            done.push(Rule {
                                rel: adorned_rel,
                                head: args.clone(),
                                body: vec![Goal { polarity: Polarity::Positive, rel: goal.rel, args }],
                                vars,
                            });
                        }
                        (demand_rel, adorned_rel)
                    }
                };
                done.push(Rule {
                    rel: demand_rel,
                    head: positions.iter().map(|p| goal.args[*p].clone()).collect(),
                    body: rule.body[..at].to_vec(),
                    vars: rule.vars.clone(),
                });
                rule.body[at].rel = adorned_rel;
            }
            if positive {
                for arg in &goal.args {
                    if let Arg::Var(v) = arg {
                        bound.insert(v.0);
                    }
                }
            }
        }
        done.push(rule);
    }
    if !queue.is_empty() {
        tracing::error!(target: "dl8::eval", left = queue.len(), "demand rewrite budget exhausted");
    }
    out.rules = done;
    (out, hidden)
}

/// One twin rule per `intern` call: head is the call, body is the prefix that
/// binds it, so the view holds the `kernel(intern)/3` rows Rust records.
fn with_intern_rows(u: &mut Universe, program: &Program) -> Program {
    // The twin heads a non-kernel name; `read` maps it back to `kernel(intern)`.
    let intern_rel = {
        let name = u.atom(INTERN_ROWS);
        u.compound("ref", vec![name])
    };
    let mut out = program.clone();
    for (index, rule) in program.rules.iter().enumerate() {
        let mut bound: HashSet<u32> = HashSet::new();
        for (at, goal) in rule.body.iter().enumerate() {
            let is_intern = goal.polarity == Polarity::Positive
                && Kernel::of(u, goal.rel) == Some(Kernel::Intern)
                && goal.args.len() == 3;
            let known = |arg: &Arg| match arg {
                Arg::Var(v) => bound.contains(&v.0),
                Arg::Ground(_) => true,
                _ => false,
            };
            if is_intern && known(&goal.args[0]) && known(&goal.args[1]) {
                out.rules.push(Rule {
                    rel: intern_rel,
                    head: goal.args.clone(),
                    body: rule.body[..=at].to_vec(),
                    vars: rule.vars.clone(),
                });
            } else if is_intern && known(&goal.args[2]) {
                // Rust matches a deconstructing `intern` against the stored
                // `kernel(intern)` rows; the twin product is that table.
                out.rules[index].body[at].rel = intern_rel;
            }
            if goal.polarity == Polarity::Positive {
                for arg in &goal.args {
                    if let Arg::Var(v) = arg {
                        bound.insert(v.0);
                    }
                }
            }
        }
    }
    out
}

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

/// The engine of the last call, kept while the rule set stays the same: a
/// macrotime wave or a comptime round then pays a seed delta, not a view.
struct Cached {
    rules: u64,
    arena: Arc<Mutex<Universe>>,
    engine: SqliteEvaluate,
    seeds: Vec<Row>,
}

thread_local! {
    static CACHE: std::cell::RefCell<Option<Cached>> = const { std::cell::RefCell::new(None) };
}

fn rules_fingerprint(program: &Program) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", program.rules).hash(&mut hasher);
    let mut served: Vec<u32> = program.served.iter().map(|rel| rel.0).collect();
    served.sort_unstable();
    served.hash(&mut hasher);
    hasher.finish()
}

fn seed_key(row: &Row) -> (u32, Vec<u32>) {
    (row.rel.0, row.args.iter().map(|term| term.0).collect())
}

fn sorted_seeds(seeds: &[Row]) -> Vec<Row> {
    let mut out: Vec<Row> = seeds.to_vec();
    out.sort_by_key(seed_key);
    out.dedup();
    out
}

pub fn evaluate_sqlite(
    u: &mut Universe,
    program: &Program,
    fx: &mut dyn FnMut(super::Trace),
) -> super::evaluate::Closure {
    let mut program = program.clone();
    let nil = nil_seed(u);
    program.seeds.push(nil);
    let before = u.terms.len();
    let rules = rules_fingerprint(&program);
    tracing::debug!(target: "dl8::eval", rules, rule_count = program.rules.len(), seeds = program.seeds.len(), "evaluate_sqlite");
    let seeds = sorted_seeds(&program.seeds);
    let taken = CACHE.with(|cache| cache.borrow_mut().take());
    let (arena, run) = match taken {
        // A retraction rebuilds: sqlite_ivm's delete path and the plain-table
        // rounds do not yet agree with a fresh view (`DL8_EVAL_CHECK=1`).
        Some(mut cached)
            if cached.rules == rules
                && cached.seeds.iter().all(|row| seeds.binary_search_by_key(&seed_key(row), seed_key).is_ok()) =>
        {
            {
                let mut guard = cached.arena.lock().expect("eval arena poisoned");
                *guard = std::mem::take(u);
            }
            let insert: Vec<Row> = seeds
                .iter()
                .filter(|row| cached.seeds.binary_search_by_key(&seed_key(row), seed_key).is_err())
                .cloned()
                .collect();
            let delete: Vec<Row> = cached
                .seeds
                .iter()
                .filter(|row| seeds.binary_search_by_key(&seed_key(row), seed_key).is_err())
                .cloned()
                .collect();
            tracing::info!(
                target: "dl8::eval",
                inserted = insert.len(),
                deleted = delete.len(),
                "seed delta on the cached view"
            );
            let retracted = !delete.is_empty();
            let arena = cached.arena.clone();
            let run = cached
                .engine
                .cover(&insert)
                .and_then(|()| cached.engine.apply(SeedDelta { insert, delete }))
                .and_then(|_| {
                    if retracted {
                        cached.engine.reset_rounds()
                    } else {
                        cached.engine.run_rounds()
                    }
                })
                .map(|()| (cached.engine, seeds.clone()));
            (arena, run)
        }
        cached => {
            let arena = match cached {
                Some(cached) => {
                    {
                        let mut guard = cached.arena.lock().expect("eval arena poisoned");
                        *guard = std::mem::take(u);
                    }
                    cached.arena
                }
                None => Arc::new(Mutex::new(std::mem::take(u))),
            };
            let run = fresh_engine(arena.clone(), &program).map(|engine| (engine, seeds.clone()));
            (arena, run)
        }
    };
    let closure = match run {
        Ok((engine, seeds)) => {
            let read = engine.read(&[]);
            if std::env::var_os("DL8_EVAL_CHECK").is_some() {
                if let (Ok(cached), Ok(fresh)) = (
                    read.as_ref(),
                    fresh_engine(arena.clone(), &program).and_then(|fresh| fresh.read(&[])),
                ) {
                    let key = |row: &Row| (row.rel.0, row.args.iter().map(|t| t.0).collect::<Vec<_>>());
                    let a: HashSet<_> = cached.rows.iter().map(key).collect();
                    let b: HashSet<_> = fresh.rows.iter().map(key).collect();
                    let guard = arena.lock().expect("eval arena poisoned");
                    for row in b.difference(&a) {
                        tracing::warn!(target: "dl8::eval", rel = %guard.display(TermId(row.0)), "fresh-only row");
                    }
                    for row in a.difference(&b) {
                        tracing::warn!(target: "dl8::eval", rel = %guard.display(TermId(row.0)), "cached-only row");
                    }
                }
            }
            CACHE.with(|cache| {
                *cache.borrow_mut() = Some(Cached {
                    rules,
                    arena: arena.clone(),
                    engine,
                    seeds,
                });
            });
            read
        }
        Err(stop) => Err(stop),
    };
    let closure = match closure {
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
    if tracing::enabled!(target: "dl8::eval", tracing::Level::DEBUG) {
        for id in before..u.terms.len() {
            tracing::debug!(target: "dl8::eval", term = %u.display(TermId(id as u32)), "minted");
        }
    }
    fx(super::Trace::Closure {
        rows: closure.rows.len(),
    });
    closure
}

fn fresh_engine(arena: Arc<Mutex<Universe>>, program: &Program) -> Result<SqliteEvaluate, Stop> {
    let mut engine = SqliteEvaluate::connect(arena)?;
    engine.declare(program)?;
    engine.apply(SeedDelta {
        insert: program.seeds.clone(),
        delete: Vec::new(),
    })?;
    Ok(engine)
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

#[cfg(test)]
mod incremental {
    use super::*;
    use crate::_6_eval::json::program_from_json;
    use std::path::PathBuf;

    /// Every oracle program: the view declared over all seeds but the last,
    /// then the last applied, reads the same rows a fresh view over all
    /// seeds reads. Prints each fixture where sqlite_ivm's maintenance and
    /// a full recompute disagree.
    #[test]
    fn a_seed_delta_reads_like_a_fresh_view() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/eval");
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        let mut bad = Vec::new();
        for path in paths {
            let text = std::fs::read_to_string(&path).unwrap();
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            let mut u = Universe::new();
            let mut program = program_from_json(&mut u, value.get("program").unwrap_or(&value)).unwrap();
            let nil = nil_seed(&mut u);
            program.seeds.push(nil);
            if program.seeds.len() < 2 {
                continue;
            }
            let arena = Arc::new(Mutex::new(std::mem::take(&mut u)));
            let fresh = fresh_engine(arena.clone(), &program).and_then(|engine| engine.read(&[]));
            let last = program.seeds.remove(0);
            if !program.seeds.iter().any(|row| row.rel == last.rel && row.args.len() == last.args.len()) {
                continue;
            }
            let staged = fresh_engine(arena.clone(), &program).and_then(|mut engine| {
                engine.apply(SeedDelta { insert: vec![last], delete: Vec::new() })?;
                engine.run_rounds()?;
                engine.read(&[])
            });
            let key = |row: &Row| (row.rel.0, row.args.iter().map(|t| t.0).collect::<Vec<_>>());
            match (fresh, staged) {
                (Ok(fresh), Ok(staged)) => {
                    let a: HashSet<_> = fresh.rows.iter().map(key).collect();
                    let b: HashSet<_> = staged.rows.iter().map(key).collect();
                    if a != b {
                        let name = path.file_stem().unwrap().to_string_lossy().to_string();
                        println!(
                            "{name}: fresh {} staged {} fresh-only {} staged-only {}",
                            a.len(),
                            b.len(),
                            a.difference(&b).count(),
                            b.difference(&a).count()
                        );
                        bad.push(name);
                    }
                }
                (fresh, staged) => {
                    bad.push(format!("{}: {:?} / {:?}", path.display(), fresh.is_ok(), staged.is_ok()));
                }
            }
        }
        assert!(bad.is_empty(), "incremental maintenance differs on {bad:?}");
    }
}
