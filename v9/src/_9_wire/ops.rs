//! Hand-owned half of the generated wire. Each generated handler in `http_auto.rs` calls the free
//! function of the same name here; each forwards to the installed [`Wire`].

use std::sync::{Arc, OnceLock};

use crate::models::change::Change;
use crate::models::delta::Delta;
use crate::models::plan::Plan;
use crate::models::query_request::QueryRequest;
use crate::models::recipe::Recipe;
use crate::models::table::Table;
use crate::ops_auto::{CommitArgs, OpError, OpResult, PlanArgs, QueryArgs, RecipeArgs};

/// What the runtime implements to serve the wire routes.
pub trait Wire: Send + Sync {
    fn commit(&self, changes: &[Change]) -> OpResult<Delta>;
    fn query(&self, request: &QueryRequest) -> OpResult<Table>;
    fn plan(&self) -> OpResult<Plan>;
    fn recipe(&self) -> OpResult<Recipe>;
}

// The generated handlers take no axum state, so the impl is process-wide and set once.
static WIRE: OnceLock<Arc<dyn Wire>> = OnceLock::new();

/// Err when a `Wire` is already installed in this process.
pub fn install(wire: Arc<dyn Wire>) -> OpResult<()> {
    WIRE.set(wire).map_err(|_| OpError("a Wire is already installed".into()))
}

fn wire() -> OpResult<&'static Arc<dyn Wire>> {
    WIRE.get().ok_or_else(|| OpError("no Wire installed".into()))
}

pub fn commit(args: &CommitArgs) -> OpResult<Delta> {
    wire()?.commit(&args.changes)
}

pub fn query(args: &QueryArgs) -> OpResult<Table> {
    wire()?.query(&args.request)
}

pub fn plan(_args: &PlanArgs) -> OpResult<Plan> {
    wire()?.plan()
}

pub fn recipe(_args: &RecipeArgs) -> OpResult<Recipe> {
    wire()?.recipe()
}

// ops_auto.rs derives clap::Args on CommitArgs, so `--changes` needs a parser: one JSON Change.
impl std::str::FromStr for Change {
    type Err = serde_json::Error;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(text)
    }
}
