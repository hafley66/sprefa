use crate::models::change::Change;
use crate::models::query_request::QueryRequest;

#[derive(Debug)]
pub struct OpError(pub String);

impl<E: std::error::Error> From<E> for OpError {
    fn from(e: E) -> Self {
        OpError(e.to_string())
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type OpResult<T> = Result<T, OpError>;

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct CommitArgs {
  #[arg(long)]
  pub changes: Vec<Change>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct QueryArgs {
  #[command(flatten)]
  pub request: QueryRequest,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, Default)]
pub struct PlanArgs {}

#[derive(clap::Args, Debug, Clone, serde::Serialize, Default)]
pub struct RecipeArgs {}
