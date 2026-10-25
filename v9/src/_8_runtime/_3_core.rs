//! `Core`: the engine, the store, the dict and the services of one program. Clients commit Base
//! rows by name; Request rows go to their service; its answers step the engine again.

use crate::_0_id::{id_of, Dict};
use crate::_1_effects::open_db;
use crate::_2_ir::{Col, Program, RelKind};
use crate::_3_plan::{check, Plan, RelId, Row};
use crate::_4_recipe::{EngineKind, Recipe, StoreKind};
use crate::_5_ends::{
    Bounds, Cadence, Cell, Change, Delta, Diag, Diff, Engine, Event, IService, Store, Table,
};
use crate::_6_back::_1_dd::Dd;
use crate::_7_store::_1_sqlite::Sqlite;
use crate::_8_runtime::_1_any::{AnyEngine, AnyStore};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Steps per commit or tick: a request, answer, request chain that keeps producing answers
/// stops here with `Diag::ServiceRounds`.
pub const MAX_SERVICE_ROUNDS: u32 = 64;

/// One change by rel name, cells in col order.
pub type Named = (String, Vec<Cell>, Diff);

/// Where one service's rows go: its Request rel in, its Answer rel out.
pub struct Route {
    pub request: RelId,
    pub answer: Option<RelId>,
}

pub struct Core {
    pub plan: Plan,
    pub recipe: Recipe,
    pub engine: AnyEngine,
    pub store: AnyStore,
    pub dict: Dict,
    /// Parallel to `routes`.
    pub services: Vec<Box<dyn IService>>,
    pub routes: Vec<Route>,
    pub ids: HashMap<String, RelId>,
    /// The engine's bounds, kept for a rebuild after a failed step back.
    pub bounds: Bounds,
    /// Set when the reseed after a failed commit could not read the store; the next commit, tick
    /// or refresh reseeds first.
    pub stale: bool,
}

/// Replays the store into the engine (not written). Every Request rel's service must be
/// in `services`; a service no Request rel names is dropped.
pub fn build(
    recipe: Recipe,
    program: Program,
    db: &Path,
    services: Vec<Box<dyn IService>>,
    bounds: Bounds,
) -> Result<Core, Vec<Diag>> {
    let mut dict = Dict::default();
    let plan = check(program, &mut dict)?;
    let ids: HashMap<String, RelId> = plan
        .rels
        .iter()
        .enumerate()
        .map(|(id, rel)| (rel.name.clone(), id as RelId))
        .collect();
    let service_of = |rel: &RelKind| match rel {
        RelKind::Request { service } | RelKind::Answer { service } => Some(service.clone()),
        _ => None,
    };
    let mut kept = Vec::new();
    let mut routes = Vec::new();
    for service in services {
        let named = |kind: fn(&RelKind) -> bool| {
            plan.rels.iter().position(|rel| {
                kind(&rel.kind) && service_of(&rel.kind).as_deref() == Some(service.service())
            })
        };
        let Some(request) = named(|kind| matches!(kind, RelKind::Request { .. })) else {
            continue;
        };
        if routes
            .iter()
            .any(|route: &Route| route.request == request as RelId)
        {
            return Err(vec![Diag::Duplicate {
                what: "service",
                name: service.service().to_string(),
            }]);
        }
        routes.push(Route {
            request: request as RelId,
            answer: named(|kind| matches!(kind, RelKind::Answer { .. })).map(|id| id as RelId),
        });
        kept.push(service);
    }
    for (id, rel) in plan.rels.iter().enumerate() {
        if let RelKind::Request { service } = &rel.kind {
            if !routes.iter().any(|route| route.request == id as RelId) {
                return Err(vec![Diag::Unknown {
                    what: "service",
                    name: service.clone(),
                    rule: None,
                }]);
            }
        }
    }
    let store = match recipe.store {
        StoreKind::Sqlite => AnyStore::Sqlite(
            Sqlite::open(
                &plan,
                &recipe.canonical(),
                open_db(db).map_err(|error| vec![Diag::Store(error.to_string())])?,
            )
            .map_err(|diag| vec![diag])?,
        ),
    };
    let engine = match recipe.engine {
        EngineKind::Dd => AnyEngine::Dd(Dd::new(&plan, bounds).map_err(|diag| vec![diag])?),
    };
    let mut core = Core {
        plan,
        recipe,
        engine,
        store,
        dict,
        services: kept,
        routes,
        ids,
        bounds,
        stale: false,
    };
    core.open().map_err(|diag| vec![diag])?;
    Ok(core)
}

impl Core {
    /// Replays Base and Answer rows and seeds each service with its stored answers. A store that
    /// was never written takes the rows derived from no input (constant bodies) and their
    /// requests' answers as one commit; nothing is written unless that commit succeeds.
    fn open(&mut self) -> Result<(), Diag> {
        let replay = self.store.replay(&mut self.dict)?;
        for (route, service) in self.routes.iter().zip(&mut self.services) {
            let answers: Vec<Change> = replay
                .iter()
                .filter(|change| Some(change.rel) == route.answer)
                .cloned()
                .collect();
            service.seed(&answers);
        }
        if self.store.event() == 0 && replay.is_empty() {
            self.rounds(Vec::new())?;
        } else {
            self.engine.step(replay)?;
        }
        Ok(())
    }

    /// Base rows by name; Derived, Request and Answer rels are `Diag::Commit`. Each diff is +1 or
    /// -1 (`Diag::Commit` "diff" otherwise) and the commit is a set operation: per row the diffs
    /// are summed, a positive sum inserts a row the store lacks, a negative sum deletes a row the
    /// store holds, anything else changes nothing. Returns every rel's change over all rounds of
    /// this commit, consolidated; a commit that changes nothing returns the last event.
    pub fn commit(&mut self, changes: Vec<Named>) -> Result<(Event, Vec<Named>), Diag> {
        self.fresh_services()?;
        let mut sums: BTreeMap<RelId, BTreeMap<Row, Diff>> = BTreeMap::new();
        let mut texts = Vec::new();
        for (index, (rel, cells, diff)) in changes.iter().enumerate() {
            let refuse = |problem, found: String| Diag::Commit {
                change: index,
                rel: rel.clone(),
                problem,
                found,
            };
            let Some(&id) = self.ids.get(rel) else {
                return Err(refuse("unknown rel", rel.clone()));
            };
            let declared = &self.plan.rels[id as usize];
            if declared.kind != RelKind::Base {
                return Err(refuse("kind", format!("{:?}", declared.kind)));
            }
            if cells.len() != declared.cols.len() {
                return Err(refuse(
                    "arity",
                    format!("{} cells for {} cols", cells.len(), declared.cols.len()),
                ));
            }
            if !matches!(diff, 1 | -1) {
                return Err(refuse("diff", diff.to_string()));
            }
            let mut row = Row::with_capacity(cells.len());
            for (slot, (cell, col)) in cells.iter().zip(&declared.cols).enumerate() {
                row.push(match (cell, col) {
                    (Cell::Text(value), Col::Text) => {
                        texts.push(value.as_str());
                        id_of(value.as_bytes())
                    }
                    (Cell::Int(value), Col::Int) => *value as u64,
                    (cell, col) => {
                        return Err(refuse("cell type", format!("col {slot} {col:?}: {cell:?}")))
                    }
                });
            }
            *sums.entry(id).or_default().entry(row).or_default() += diff;
        }
        let mut input = Vec::new();
        for (rel, rows) in sums {
            let rows: Vec<(Row, Diff)> = rows.into_iter().filter(|(_, sum)| *sum != 0).collect();
            let asked: Vec<&Row> = rows.iter().map(|(row, _)| row).collect();
            let present: HashSet<Row> = self.store.present(rel, &asked)?.into_iter().collect();
            input.extend(
                rows.into_iter()
                    .filter(|(row, sum)| (*sum > 0) != present.contains(row))
                    .map(|(row, sum)| Change {
                        rel,
                        row,
                        diff: sum.signum(),
                    }),
            );
        }
        for text in texts {
            self.dict.intern(text.as_bytes());
        }
        if input.is_empty() {
            return Ok((self.store.event(), Vec::new()));
        }
        self.rounds(input)
    }

    /// Polls Continuing services; their answers step the engine. `None` when the consolidated
    /// change is empty.
    pub fn tick(&mut self) -> Result<Option<(Event, Vec<Named>)>, Diag> {
        self.fresh_services()?;
        let answers = self.poll(|_, cadence| cadence == Cadence::Continuing);
        let answers = match answers {
            Ok(answers) => answers,
            Err(diag) => {
                self.stale = self.reseed().is_err();
                return Err(diag);
            }
        };
        if answers.is_empty() {
            return Ok(None);
        }
        let (event, changes) = self.rounds(answers)?;
        Ok((!changes.is_empty()).then_some((event, changes)))
    }

    /// Re-asks every stored Request row of the Once service `service` and commits the answers it
    /// polls. A service seeded at open answers the difference against the stored Answer rows, so
    /// a changed answer set moves the stored one; `None` when the consolidated change is empty.
    pub fn refresh(&mut self, service: &str) -> Result<Option<(Event, Vec<Named>)>, Diag> {
        self.fresh_services()?;
        let Some(index) = self
            .services
            .iter()
            .position(|known| known.service() == service)
        else {
            return Err(Diag::Unknown {
                what: "service",
                name: service.to_string(),
                rule: None,
            });
        };
        if self.services[index].cadence() != Cadence::Once {
            return Err(Diag::Service {
                service: service.to_string(),
                message: "refresh re-asks Once services only".into(),
            });
        }
        let request = self.routes[index].request;
        let asks: Vec<Change> = self
            .store
            .rows(request)?
            .into_iter()
            .map(|row| Change {
                rel: request,
                row,
                diff: 1,
            })
            .collect();
        if !asks.is_empty() {
            self.services[index].call(&asks, &self.dict);
        }
        let answers = self.poll(|at, _| at == index);
        let answers = match answers {
            Ok(answers) => answers,
            Err(diag) => {
                self.stale = self.reseed().is_err();
                return Err(diag);
            }
        };
        if answers.is_empty() {
            return Ok(None);
        }
        let (event, changes) = self.rounds(answers)?;
        Ok((!changes.is_empty()).then_some((event, changes)))
    }

    pub fn query(&self, sql: &str) -> Result<Table, Diag> {
        self.store.query(sql)
    }

    /// One atomic commit: step, route requests, poll every service, again while answers come, at
    /// most `MAX_SERVICE_ROUNDS` steps; then one store write of the consolidated change. On any
    /// failure the engine steps back by the negated input of every round, nothing is stored and
    /// every service is reseeded from the stored Answer rows.
    fn rounds(&mut self, input: Vec<Change>) -> Result<(Event, Vec<Named>), Diag> {
        let mut fed: BTreeMap<(RelId, Row), Diff> = BTreeMap::new();
        let mut sums: BTreeMap<(RelId, Row), Diff> = BTreeMap::new();
        let written = self
            .settle(input, &mut fed, &mut sums)
            .and_then(|()| self.write(sums));
        match written {
            Ok(done) => Ok(done),
            Err(diag) => {
                let undone = self.step_back(fed);
                self.stale = true;
                if self.reseed().is_ok() {
                    self.stale = false;
                }
                Err(undone.err().unwrap_or(diag))
            }
        }
    }

    /// The rounds of one commit; `fed` sums the input of every step that succeeded, `sums` its
    /// output.
    fn settle(
        &mut self,
        mut input: Vec<Change>,
        fed: &mut BTreeMap<(RelId, Row), Diff>,
        sums: &mut BTreeMap<(RelId, Row), Diff>,
    ) -> Result<(), Diag> {
        for _ in 0..MAX_SERVICE_ROUNDS {
            let delta = self.engine.step(input.clone())?;
            for change in input {
                *fed.entry((change.rel, change.row)).or_default() += change.diff;
            }
            for change in &delta.changes {
                *sums.entry((change.rel, change.row.clone())).or_default() += change.diff;
            }
            self.route(&delta);
            input = self.poll(|_, _| true)?;
            if input.is_empty() {
                return Ok(());
            }
        }
        Err(Diag::ServiceRounds {
            max_rounds: MAX_SERVICE_ROUNDS,
        })
    }

    /// Writes a commit's consolidated change as the next event; an empty change writes nothing
    /// and keeps the last event. A failed write leaves the dict entries fresh for the next one.
    fn write(&mut self, sums: BTreeMap<(RelId, Row), Diff>) -> Result<(Event, Vec<Named>), Diag> {
        let delta = Delta {
            event: self.store.event() + 1,
            changes: sums
                .into_iter()
                .filter(|(_, diff)| *diff != 0)
                .map(|((rel, row), diff)| Change { rel, row, diff })
                .collect(),
        };
        if delta.changes.is_empty() {
            return Ok((self.store.event(), Vec::new()));
        }
        let fresh = self.dict.fresh.clone();
        if let Err(diag) = self.store.write(&delta, &self.dict.take_new()) {
            self.dict.fresh = fresh;
            return Err(diag);
        }
        let changes = delta
            .changes
            .iter()
            .map(|change| self.named(change.rel, &change.row, change.diff))
            .collect();
        Ok((delta.event, changes))
    }

    /// Steps the engine back by `fed` negated. That one step can need more rounds than the bound
    /// (a retraction cascade); then a fresh engine replays the store. A Diag from the replay means
    /// the engine no longer matches the store.
    fn step_back(&mut self, fed: BTreeMap<(RelId, Row), Diff>) -> Result<(), Diag> {
        let undo: Vec<Change> = fed
            .into_iter()
            .filter(|(_, diff)| *diff != 0)
            .map(|((rel, row), diff)| Change {
                rel,
                row,
                diff: -diff,
            })
            .collect();
        if undo.is_empty() || self.engine.step(undo).is_ok() {
            return Ok(());
        }
        let mut engine = match self.recipe.engine {
            EngineKind::Dd => AnyEngine::Dd(Dd::new(&self.plan, self.bounds)?),
        };
        engine.step(self.store.replay(&mut self.dict)?)?;
        self.engine = engine;
        Ok(())
    }

    /// Reseeds the services when a failed commit left them `stale`.
    fn fresh_services(&mut self) -> Result<(), Diag> {
        if self.stale {
            self.reseed()?;
            self.stale = false;
        }
        Ok(())
    }

    /// Hands every service its stored Answer rows, replacing its memory.
    fn reseed(&mut self) -> Result<(), Diag> {
        for (route, service) in self.routes.iter().zip(&mut self.services) {
            let answers: Vec<Change> = match route.answer {
                Some(answer) => self
                    .store
                    .rows(answer)?
                    .into_iter()
                    .map(|row| Change {
                        rel: answer,
                        row,
                        diff: 1,
                    })
                    .collect(),
                None => Vec::new(),
            };
            service.seed(&answers);
        }
        Ok(())
    }

    fn route(&mut self, delta: &Delta) {
        for (route, service) in self.routes.iter().zip(&mut self.services) {
            let requests: Vec<Change> = delta
                .changes
                .iter()
                .filter(|change| change.rel == route.request)
                .cloned()
                .collect();
            if !requests.is_empty() {
                service.call(&requests, &self.dict);
            }
        }
    }

    /// Answers of the services `pick` takes by index and cadence, each set to its service's
    /// Answer rel. A service that reports errors stops the poll with the first.
    fn poll(&mut self, pick: impl Fn(usize, Cadence) -> bool) -> Result<Vec<Change>, Diag> {
        let mut answers = Vec::new();
        for (index, (route, service)) in self.routes.iter().zip(&mut self.services).enumerate() {
            if !pick(index, service.cadence()) {
                continue;
            }
            let mut out = Vec::new();
            service.poll(&mut self.dict, &mut out);
            let errors = service.errors();
            let failed = |message: String| Diag::Service {
                service: service.service().to_string(),
                message,
            };
            if let Some(error) = errors.into_iter().next() {
                return Err(failed(error));
            }
            if out.is_empty() {
                continue;
            }
            let Some(answer) = route.answer else {
                return Err(failed("answers but the program has no Answer rel".into()));
            };
            let width = self.plan.rels[answer as usize].cols.len();
            for mut change in out {
                if change.row.len() != width {
                    return Err(failed(format!(
                        "answer row {:?} has width {}, expected {width}",
                        change.row,
                        change.row.len()
                    )));
                }
                change.rel = answer;
                answers.push(change);
            }
        }
        Ok(answers)
    }

    /// Text through the dict, Int from its bits.
    pub fn named(&self, rel: RelId, row: &Row, diff: Diff) -> Named {
        let declared = &self.plan.rels[rel as usize];
        let cells = declared
            .cols
            .iter()
            .zip(row)
            .map(|(col, value)| match col {
                Col::Text => self.dict.bytes.get(value).map_or(Cell::Null, |bytes| {
                    Cell::Text(String::from_utf8_lossy(bytes).into_owned())
                }),
                Col::Int => Cell::Int(*value as i64),
            })
            .collect();
        (declared.name.clone(), cells, diff)
    }
}
