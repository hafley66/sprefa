use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RelKind {
  Base,
  Derived,
  Request,
  Answer,
}
