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
    /// `SQLite transaction<'_>` borrow across engine calls. The outermost
    /// boundary owns `BEGIN IMMEDIATE` and its matching commit/rollback. When a
    /// caller already owns a wider transaction, the boundary only runs `work`;
    /// it never commits or rolls back its caller. Panics roll back an owned
    /// generation and then resume unwinding unchanged.
    pub(crate) fn with_semantic_generation<T>(
        &mut self,
        work: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if !self.db.is_autocommit() {
            return work(self);
        }

        self.db.begin_immediate()?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(self))) {
            Ok(Ok(value)) => {
                if let Err(error) = self.db.commit() {
                    let _ = self.db.rollback();
                    return Err(error);
                }
                Ok(value)
            }
            Ok(Err(error)) => {
                let _ = self.db.rollback();
                Err(error)
            }
            Err(payload) => {
                let _ = self.db.rollback();
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
            .query_one(
                "generation_boundary",
                "SELECT COUNT(*) FROM generation_boundary",
                &[],
                |row| Ok(row.get(0)?),
            )
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
        assert!(engine.db.is_autocommit());
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
        assert!(engine.db.is_autocommit());
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
        assert!(engine.db.is_autocommit());
        assert_eq!(generation_rows(&engine), 0);
    }

    #[test]
    fn semantic_generation_never_owns_callers_transaction() {
        let mut engine = generation_engine();
        engine.db.begin_immediate().unwrap();
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
            !engine.db.is_autocommit(),
            "boundary must leave caller transaction open"
        );
        assert_eq!(
            generation_rows(&engine),
            2,
            "boundary must not roll back its caller"
        );
        engine.db.rollback().unwrap();
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
            .query_one(
                "generation_bulk",
                "SELECT COUNT(*) FROM generation_bulk",
                &[],
                |row| Ok(row.get(0)?),
            )
            .unwrap();
        assert_eq!(
            persisted, 0,
            "insert_rows must not commit its generation owner"
        );
    }
}
