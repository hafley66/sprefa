//! The thread that owns a `Core` (the DD worker is not Send) and the `Handle` that reaches it over
//! a channel. `Handle` is the `Wire` the generated HTTP routes call.

use crate::_1_effects::spawn;
use crate::_2_ir::{Col, RelKind};
use crate::_5_ends::{Cell, Diag, Event, Table};
use crate::_8_runtime::_3_core::{Core, Named};
use crate::models;
use crate::ops::Wire;
use crate::ops_auto::{OpError, OpResult};
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
                        col_type: match col {
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

fn failed(diag: Diag) -> OpError {
    OpError(format!("{diag:?}"))
}

/// Exactly one of `text` and `int`; neither is `Cell::Null`, which every col type refuses.
fn from_wire(cell: &models::cell::Cell) -> OpResult<Cell> {
    match cell {
        models::cell::Cell {
            text: Some(text),
            int: None,
        } => Ok(Cell::Text(text.clone())),
        models::cell::Cell {
            text: None,
            int: Some(int),
        } => Ok(Cell::Int(*int)),
        models::cell::Cell {
            text: None,
            int: None,
        } => Ok(Cell::Null),
        both => Err(OpError(format!("a cell holds text and int: {both:?}"))),
    }
}

/// The wire carries text and int; a Real or Blob SQL value is an error.
fn to_wire(cell: &Cell) -> OpResult<models::cell::Cell> {
    let (text, int) = match cell {
        Cell::Null => (None, None),
        Cell::Int(int) => (None, Some(*int)),
        Cell::Text(text) => (Some(text.clone()), None),
        other => return Err(OpError(format!("no wire form for {other:?}"))),
    };
    Ok(models::cell::Cell { text, int })
}

impl Wire for Handle {
    fn commit(&self, changes: &[models::change::Change]) -> OpResult<models::delta::Delta> {
        let named = changes
            .iter()
            .map(|change| {
                Ok((
                    change.rel.clone(),
                    change
                        .cells
                        .iter()
                        .map(from_wire)
                        .collect::<OpResult<_>>()?,
                    change.diff as isize,
                ))
            })
            .collect::<OpResult<Vec<Named>>>()?;
        let (event, changes) = Handle::commit(self, named).map_err(failed)?;
        Ok(models::delta::Delta {
            event,
            changes: changes
                .into_iter()
                .map(|(rel, cells, diff)| {
                    Ok(models::change::Change {
                        rel,
                        cells: cells.iter().map(to_wire).collect::<OpResult<_>>()?,
                        diff: diff as i64,
                    })
                })
                .collect::<OpResult<_>>()?,
        })
    }

    fn query(
        &self,
        request: &models::query_request::QueryRequest,
    ) -> OpResult<models::table::Table> {
        let table = Handle::query(self, &request.sql).map_err(failed)?;
        Ok(models::table::Table {
            columns: table.columns,
            rows: table
                .rows
                .iter()
                .map(|row| row.iter().map(to_wire).collect())
                .collect::<OpResult<_>>()?,
        })
    }

    fn plan(&self) -> OpResult<models::plan::Plan> {
        Ok(self.plan.clone())
    }

    fn recipe(&self) -> OpResult<models::recipe::Recipe> {
        Ok(self.recipe.clone())
    }
}
