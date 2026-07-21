//! Name resolution + variable binding. Stub. Resolves rel names, column names,
//! and binds Term::Var occurrences; produces the ref-graph edges analyze consumes.

use crate::_0_key::{RelId, VarId};

pub struct Resolved {
    pub rel_defs: Vec<RelId>,
    pub var_binds: Vec<VarId>,
}
