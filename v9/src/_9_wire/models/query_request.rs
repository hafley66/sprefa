use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args, Default)]
pub struct QueryRequest {
  #[arg(long)]
  pub sql: String,
}
