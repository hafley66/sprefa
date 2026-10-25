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
use crate::_7_store::_0_sql::quote;
use crate::_7_store::_1_sqlite::Sqlite;
use crate::_8_runtime::_1_any::{AnyEngine, AnyStore};
use std::collections::{BTreeMap, HashMap};
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
}

/// Replays the store into the engine (event 1, not written). Every Request rel's service must be
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
    };
    core.open().map_err(|diag| vec![diag])?;
    Ok(core)
}

impl Core {
    /// Replays Base and Answer rows and seeds each service with its stored answers. An empty
    /// store with rows derived from no input (constant bodies) writes them once and serves their
    /// requests.
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
        let empty = replay.is_empty();
        let delta = self.engine.step(replay)?;
        let Some(first) = delta.changes.first().filter(|_| empty) else {
            return Ok(());
        };
        let table = self.store.query(&format!(
            "SELECT count(*) FROM {}",
            quote(&self.plan.rels[first.rel as usize].name)
        ))?;
        if table.rows != [[Cell::Int(0)]] {
            return Ok(());
        }
        self.write(&delta)?;
        self.route(&delta);
        let answers = self.poll(|_, _| true)?;
        if !answers.is_empty() {
            self.rounds(answers)?;
        }
        Ok(())
    }

    /// Base rows by name; Derived, Request and Answer rels are `Diag::Commit`. Returns every
    /// rel's change over all rounds of this commit, consolidated.
    pub fn commit(&mut self, changes: Vec<Named>) -> Result<(Event, Vec<Named>), Diag> {
        let mut input = Vec::with_capacity(changes.len());
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
            input.push(Change {
                rel: id,
                row,
                diff: *diff,
            });
        }
        for text in texts {
            self.dict.intern(text.as_bytes());
        }
        self.rounds(input)
    }

    /// Polls Continuing services; their answers step the engine.
    pub fn tick(&mut self) -> Result<Option<(Event, Vec<Named>)>, Diag> {
        let answers = self.poll(|_, cadence| cadence == Cadence::Continuing)?;
        if answers.is_empty() {
            return Ok(None);
        }
        self.rounds(answers).map(Some)
    }

    /// Re-asks every stored Request row of the Once service `service` and commits the answers it
    /// polls. A service seeded at open answers the difference against the stored Answer rows, so
    /// a changed answer set moves the stored one; `None` when nothing changed.
    pub fn refresh(&mut self, service: &str) -> Result<Option<(Event, Vec<Named>)>, Diag> {
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
        let table = self.store.query(&format!(
            "SELECT * FROM {}",
            quote(&self.plan.rels[request as usize].name)
        ))?;
        let mut asks = Vec::with_capacity(table.rows.len());
        for cells in table.rows {
            let row = cells
                .into_iter()
                .map(|cell| match cell {
                    Cell::Int(value) => Ok(value as u64),
                    other => Err(Diag::Store(format!("request row cell {other:?}"))),
                })
                .collect::<Result<Row, Diag>>()?;
            asks.push(Change {
                rel: request,
                row,
                diff: 1,
            });
        }
        if !asks.is_empty() {
            self.services[index].call(&asks, &self.dict);
        }
        let answers = self.poll(|at, _| at == index)?;
        if answers.is_empty() {
            return Ok(None);
        }
        self.rounds(answers).map(Some)
    }

    pub fn query(&self, sql: &str) -> Result<Table, Diag> {
        self.store.query(sql)
    }

    /// step, write, route requests, poll every service; again while answers come, at most
    /// `MAX_SERVICE_ROUNDS` steps.
    fn rounds(&mut self, mut input: Vec<Change>) -> Result<(Event, Vec<Named>), Diag> {
        let mut sums: BTreeMap<(RelId, Row), Diff> = BTreeMap::new();
        for _ in 0..MAX_SERVICE_ROUNDS {
            let delta = self.engine.step(input.clone())?;
            if let Err(diag) = self.write(&delta) {
                let undo = input
                    .into_iter()
                    .map(|change| Change {
                        diff: -change.diff,
                        ..change
                    })
                    .collect();
                self.engine.step(undo)?;
                return Err(diag);
            }
            self.route(&delta);
            for change in &delta.changes {
                *sums.entry((change.rel, change.row.clone())).or_default() += change.diff;
            }
            input = self.poll(|_, _| true)?;
            if input.is_empty() {
                let changes = sums
                    .into_iter()
                    .filter(|(_, diff)| *diff != 0)
                    .map(|((rel, row), diff)| self.named(rel, &row, diff))
                    .collect();
                return Ok((delta.event, changes));
            }
        }
        Err(Diag::ServiceRounds {
            max_rounds: MAX_SERVICE_ROUNDS,
        })
    }

    /// A failed write leaves the dict entries fresh for the next one.
    fn write(&mut self, delta: &Delta) -> Result<usize, Diag> {
        let fresh = self.dict.fresh.clone();
        let written = self.store.write(delta, &self.dict.take_new());
        if written.is_err() {
            self.dict.fresh = fresh;
        }
        written
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
