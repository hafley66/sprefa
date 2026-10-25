use serde::Deserialize;
use serde::Serialize;

use crate::_9_wire::models::relation::Relation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
  pub relations: Vec<Relation>,
}
