//! The thread that owns a `Core` (the DD worker is not Send) and the `Handle` that reaches it over
//! a channel. `Handle` is the `Wire` the generated HTTP router serves as its axum state.

use crate::_1_effects::spawn;
use crate::_2_ir::{Col, RelKind};
use crate::_5_ends::{Cell, Diag, Event, Table};
use crate::_8_runtime::_3_core::{Core, Named};
use crate::_9_wire::models;
use crate::_9_wire::models::cell::Cell as WireCell;
use crate::_9_wire::models::commit_refused::CommitRefused;
use crate::_9_wire::models::server_error::ServerError;
use crate::_9_wire::models::sql_rejected::SqlRejected;
use crate::_9_wire::ops_auto::{
    CommitArgs, OpError, OpResult, PlanArgs, QueryArgs, RecipeArgs, Wire,
};
use std::sync::mpsc::{channel, Sender};

type Reply<T> = Sender<Result<T, Diag>>;

pub enum Command {
    Commit(Vec<Named>, Reply<(Event, Vec<Named>)>),
    Query(String, Reply<Table>),
    Tick(Reply<Option<(Event, Vec<Named>)>>),
}

/// Plan and recipe are fixed for the Core's life, so they are read once at start.
pub struct Handle {
    pub commands: Sender<Command>,
    pub plan: models::plan::Plan,
    pub recipe: models::recipe::Recipe,
}

/// Runs `make` on a new thread; that thread serves commands until every `Handle` clone drops.
pub fn start(
    make: impl FnOnce() -> Result<Core, Vec<Diag>> + Send + 'static,
) -> Result<Handle, Vec<Diag>> {
    let (commands, inbox) = channel::<Command>();
    let (ready, started) = channel();
    spawn("v9-core", move || {
        let mut core = match make() {
            Ok(core) => core,
            Err(diags) => return drop(ready.send(Err(diags))),
        };
        let _ = ready.send(Ok((wire_plan(&core), core.recipe.canonical())));
        for command in inbox {
            // A caller that stopped waiting drops its receiver; the reply is discarded.
            match command {
                Command::Commit(changes, reply) => drop(reply.send(core.commit(changes))),
                Command::Query(sql, reply) => drop(reply.send(core.query(&sql))),
                Command::Tick(reply) => drop(reply.send(core.tick())),
            }
        }
    })
    .map_err(|error| vec![stopped(error.to_string())])?;
    let (plan, canonical) = started
        .recv()
        .map_err(|error| vec![stopped(error.to_string())])??;
    Ok(Handle {
        commands,
        plan,
        recipe: models::recipe::Recipe { canonical },
    })
}

fn stopped(message: String) -> Diag {
    Diag::Service {
        service: "core".into(),
        message,
    }
}

impl Handle {
    fn ask<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Diag> {
        let (reply, answer) = channel();
        self.commands
            .send(command(reply))
            .map_err(|_| stopped("core thread stopped".into()))?;
        answer
            .recv()
            .map_err(|_| stopped("core thread stopped".into()))?
    }

    pub fn commit(&self, changes: Vec<Named>) -> Result<(Event, Vec<Named>), Diag> {
        self.ask(|reply| Command::Commit(changes, reply))
    }

    pub fn query(&self, sql: &str) -> Result<Table, Diag> {
        self.ask(|reply| Command::Query(sql.to_string(), reply))
    }

    pub fn tick(&self) -> Result<Option<(Event, Vec<Named>)>, Diag> {
        self.ask(Command::Tick)
    }
}

fn wire_plan(core: &Core) -> models::plan::Plan {
    let relations = core
        .plan
        .program
        .rels
        .iter()
        .map(|rel| {
            let (kind, service) = match &rel.kind {
                RelKind::Base => (models::rel_kind::RelKind::Base, None),
                RelKind::Derived => (models::rel_kind::RelKind::Derived, None),
                RelKind::Request { service } => {
                    (models::rel_kind::RelKind::Request, Some(service.clone()))
                }
                RelKind::Answer { service } => {
                    (models::rel_kind::RelKind::Answer, Some(service.clone()))
                }
            };
            models::relation::Relation {
                name: rel.name.clone(),
                cols: rel
                    .cols
                    .iter()
                    .map(|(name, col)| models::column::Column {
                        name: name.clone(),
                        r#type: match col {
                            Col::Text => models::col_type::ColType::Text,
                            Col::Int => models::col_type::ColType::Int,
                        },
                    })
                    .collect(),
                kind,
                service,
            }
        })
        .collect();
    models::plan::Plan { relations }
}

/// A failure the client did not cause: 500 `ServerError`.
fn server_error(diag: Diag) -> OpError {
    ServerError {
        error: format!("{diag:?}"),
    }
    .into()
}

/// A commit the runtime refuses or a check failure is the client's: 400. The rest is 500.
fn commit_failed(diag: Diag) -> OpError {
    match diag {
        Diag::Commit { .. }
        | Diag::Unknown { .. }
        | Diag::Duplicate { .. }
        | Diag::Arity { .. }
        | Diag::Type { .. }
        | Diag::Unbound { .. }
        | Diag::HeadKind { .. }
        | Diag::AnswerShape { .. }
        | Diag::NegationCycle { .. } => CommitRefused {
            diag: format!("{diag:?}"),
        }
        .into(),
        other => server_error(other),
    }
}

/// On a query every `Diag::Store` is the client's SQL: rejected SQL, a row cap or a time budget
/// the query ran over, an answer cell with no wire form. 400. The rest is 500.
fn query_failed(diag: Diag) -> OpError {
    match diag {
        Diag::Store(_) => SqlRejected {
            diag: format!("{diag:?}"),
        }
        .into(),
        other => server_error(other),
    }
}

fn from_wire(cell: &Option<WireCell>) -> Cell {
    match cell {
        None => Cell::Null,
        Some(WireCell::text(text)) => Cell::Text(text.clone()),
        Some(WireCell::int(int)) => Cell::Int(*int),
        Some(WireCell::real(real)) => Cell::Real(*real),
    }
}

/// A Blob SQL value has no wire form; the `Diag::Store` names it. From a client query it is the
/// client's SQL (400); from a commit's delta it is a runtime defect (500).
fn to_wire(cell: &Cell) -> Result<Option<WireCell>, Diag> {
    Ok(match cell {
        Cell::Null => None,
        Cell::Int(int) => Some(WireCell::int(*int)),
        Cell::Real(real) => Some(WireCell::real(*real)),
        Cell::Text(text) => Some(WireCell::text(text.clone())),
        Cell::Blob(_) => return Err(Diag::Store(format!("no wire form for {cell:?}"))),
    })
}

impl Wire for Handle {
    fn commit(&self, args: &CommitArgs) -> OpResult<models::delta::Delta> {
        let named = args
            .changes
            .iter()
            .map(|change| {
                (
                    change.rel.clone(),
                    change.cells.iter().map(from_wire).collect(),
                    change.diff as isize,
                )
            })
            .collect();
        let (event, changes) = Handle::commit(self, named).map_err(commit_failed)?;
        Ok(models::delta::Delta {
            event,
            changes: changes
                .into_iter()
                .map(|(rel, cells, diff)| {
                    Ok(models::change::Change {
                        rel,
                        cells: cells
                            .iter()
                            .map(to_wire)
                            .collect::<Result<_, _>>()
                            .map_err(server_error)?,
                        diff: diff as i64,
                    })
                })
                .collect::<OpResult<_>>()?,
        })
    }

    fn query(&self, args: &QueryArgs) -> OpResult<models::table::Table> {
        let table = Handle::query(self, &args.request.sql).map_err(query_failed)?;
        Ok(models::table::Table {
            columns: table.columns,
            rows: table
                .rows
                .iter()
                .map(|row| row.iter().map(to_wire).collect())
                .collect::<Result<_, _>>()
                .map_err(query_failed)?,
        })
    }

    fn plan(&self, _args: &PlanArgs) -> OpResult<models::plan::Plan> {
        Ok(self.plan.clone())
    }

    fn recipe(&self, _args: &RecipeArgs) -> OpResult<models::recipe::Recipe> {
        Ok(self.recipe.clone())
    }
}
