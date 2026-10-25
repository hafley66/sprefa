use crate::_9_wire::models::change::Change;
use crate::_9_wire::models::commit_refused::CommitRefused;
use crate::_9_wire::models::delta::Delta;
use crate::_9_wire::models::plan::Plan;
use crate::_9_wire::models::query_request::QueryRequest;
use crate::_9_wire::models::recipe::Recipe;
use crate::_9_wire::models::sql_rejected::SqlRejected;
use crate::_9_wire::models::table::Table;

#[derive(Debug)]
pub enum OpError {
  Internal(String),
  CommitRefused(CommitRefused),
  SqlRejected(SqlRejected),
}

impl<E: std::error::Error> From<E> for OpError {
    fn from(e: E) -> Self {
        OpError::Internal(e.to_string())
    }
}

impl From<CommitRefused> for OpError {
    fn from(e: CommitRefused) -> Self {
        OpError::CommitRefused(e)
    }
}

impl From<SqlRejected> for OpError {
    fn from(e: SqlRejected) -> Self {
        OpError::SqlRejected(e)
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Internal(message) => f.write_str(message),
            OpError::CommitRefused(body) => f.write_str(&serde_json::to_string(body).map_err(|_| std::fmt::Error)?),
            OpError::SqlRejected(body) => f.write_str(&serde_json::to_string(body).map_err(|_| std::fmt::Error)?),
        }
    }
}

pub type OpResult<T> = Result<T, OpError>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitArgs {
  pub changes: Vec<Change>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryArgs {
  pub request: QueryRequest,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct PlanArgs {}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct RecipeArgs {}

pub trait Wire: Send + Sync + 'static {
  fn commit(&self, args: &CommitArgs) -> OpResult<Delta>;
  fn query(&self, args: &QueryArgs) -> OpResult<Table>;
  fn plan(&self, args: &PlanArgs) -> OpResult<Plan>;
  fn recipe(&self, args: &RecipeArgs) -> OpResult<Recipe>;
}
