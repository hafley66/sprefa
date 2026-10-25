use serde::Deserialize;
use serde::Serialize;

use crate::models::change::Change;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
  pub event: u64,
  pub changes: Vec<Change>,
}
