//! Atomic SQLite apply boundary for one semantic generation.

use anyhow::Result;

use crate::engine::Engine;

// The production ReadyGeneration -> CommittedGeneration adapter is
// intentionally absent. It must own an outermost transaction, revalidate the
// prepared BaseStamp after BEGIN IMMEDIATE, apply the staged work, and only then
// mint CommittedGeneration. The legacy closure boundary below has no BaseStamp
// and therefore cannot truthfully implement that transition yet.

impl Engine {
    /// Run the SQLite-owned portion of one semantic generation atomically.
    ///
    /// This deliberately uses SQL transaction control instead of holding a
    /// `rusqlite::Transaction<'_>` borrow across engine calls. The outermost
    /// boundary owns `BEGIN IMMEDIATE` and its matching commit/rollback. When a
    /// caller already owns a wider transaction, the boundary only runs `work`;
    /// it never commits or rolls back its caller. Panics roll back an owned
    /// generation and then resume unwinding unchanged.
    pub(crate) fn with_semantic_generation<T>(
        &mut self,
        work: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if !self.db.conn().is_autocommit() {
            return work(self);
        }

        self.db.conn().execute_batch("BEGIN IMMEDIATE")?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(self))) {
            Ok(Ok(value)) => {
                if let Err(error) = self.db.conn().execute_batch("COMMIT") {
                    let _ = self.db.conn().execute_batch("ROLLBACK");
                    return Err(error.into());
                }
                Ok(value)
            }
            Ok(Err(error)) => {
                let _ = self.db.conn().execute_batch("ROLLBACK");
                Err(error)
            }
            Err(payload) => {
                let _ = self.db.conn().execute_batch("ROLLBACK");
                std::panic::resume_unwind(payload)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Value;
    use std::path::PathBuf;

    fn generation_engine() -> Engine {
        let engine = Engine::new(crate::db::open(None).unwrap(), PathBuf::new());
        engine
            .db
            .exec("CREATE TABLE generation_boundary (value TEXT)")
            .unwrap();
        engine
    }

    fn generation_rows(engine: &Engine) -> i64 {
        engine
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM generation_boundary", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    #[test]
    fn semantic_generation_commits_success() {
        let mut engine = generation_engine();
        let value = engine
            .with_semantic_generation(|engine| {
                engine
                    .db
                    .exec("INSERT INTO generation_boundary VALUES ('committed')")?;
                Ok(7)
            })
            .unwrap();

        assert_eq!(value, 7);
        assert!(engine.db.conn().is_autocommit());
        assert_eq!(generation_rows(&engine), 1);
    }

    #[test]
    fn semantic_generation_rolls_back_error() {
        let mut engine = generation_engine();
        let result: Result<()> = engine.with_semantic_generation(|engine| {
            engine
                .db
                .exec("INSERT INTO generation_boundary VALUES ('partial')")?;
            anyhow::bail!("generation failed")
        });

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("generation failed"));
        assert!(engine.db.conn().is_autocommit());
        assert_eq!(generation_rows(&engine), 0);
    }

    #[test]
    fn semantic_generation_rolls_back_panic_and_resumes_unwind() {
        let mut engine = generation_engine();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<()> = engine.with_semantic_generation(|engine| {
                engine
                    .db
                    .exec("INSERT INTO generation_boundary VALUES ('partial')")?;
                panic!("generation panicked")
            });
        }));

        let payload = panic.expect_err("panic must resume after rollback");
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"generation panicked"));
        assert!(engine.db.conn().is_autocommit());
        assert_eq!(generation_rows(&engine), 0);
    }

    #[test]
    fn semantic_generation_never_owns_callers_transaction() {
        let mut engine = generation_engine();
        engine.db.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
        engine
            .db
            .exec("INSERT INTO generation_boundary VALUES ('before boundary')")
            .unwrap();

        let result: Result<()> = engine.with_semantic_generation(|engine| {
            engine
                .db
                .exec("INSERT INTO generation_boundary VALUES ('inside boundary')")?;
            anyhow::bail!("caller decides")
        });

        assert!(result.is_err());
        assert!(
            !engine.db.conn().is_autocommit(),
            "boundary must leave caller transaction open"
        );
        assert_eq!(
            generation_rows(&engine),
            2,
            "boundary must not roll back its caller"
        );
        engine.db.conn().execute_batch("ROLLBACK").unwrap();
        assert_eq!(generation_rows(&engine), 0);
    }

    #[test]
    fn semantic_generation_owns_chunked_insert_rows_atomically() {
        let mut engine = generation_engine();
        engine
            .db
            .exec("CREATE TABLE generation_bulk (value INTEGER PRIMARY KEY)")
            .unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|n| vec![Value::Int(n)]).collect();

        let result: Result<()> = engine.with_semantic_generation(|engine| {
            assert_eq!(
                engine
                    .db
                    .insert_rows("generation_bulk", &["value"], &rows)?,
                rows.len(),
            );
            anyhow::bail!("roll back whole generation")
        });

        assert!(result.is_err());
        let persisted: i64 = engine
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM generation_bulk", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            persisted, 0,
            "insert_rows must not commit its generation owner"
        );
    }
}
