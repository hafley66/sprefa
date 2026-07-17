//! Internal write-ledger bookkeeping. The `_write_ledger` table is created
//! directly by the meta schema and flushed once per tick; it is deliberately
//! NOT projected into a queryable built-in relation, so there is no new read
//! surface. Registering it as a `RelKind` with `bookkeeping = true` keeps it out
//! of `is_settled` so a quiet tick does not spin forever.

use anyhow::Result;

use crate::ast::RelDecl;
use crate::engine::Engine;

use super::RelKind;

pub struct WriteLedgerKind;

impl RelKind for WriteLedgerKind {
    fn rels(&self) -> &'static [&'static str] {
        &["_write_ledger"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        // Empty: `_write_ledger` is an internal meta table, not a public rel.
        vec![]
    }
    fn reserved_msg(&self) -> &'static str {
        "the internal write-ledger table"
    }
    fn refresh(&self, _eng: &Engine) -> Result<bool> {
        // The engine flushes `_write_ledger` directly at tick end.
        Ok(false)
    }
    fn bookkeeping(&self) -> bool {
        true
    }
}
