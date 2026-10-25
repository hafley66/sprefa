use serde::Deserialize;
use serde::Serialize;

use crate::models::relation::Relation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
  pub relations: Vec<Relation>,
}
