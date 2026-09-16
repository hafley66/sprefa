//! Isolated proof that one semantic generation can commit atomically.
//!
//! This module is intentionally not wired into `engine` yet.  It models the
//! transaction boundary required by source deltas and derived deltaflow before
//! production orchestration adopts it.

#[cfg(test)]
mod tests {
    use crate::db::{self, Db};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Failpoint {
        BeforeSource,
        AfterSource,
        AfterDerived,
        AfterDigest,
        BeforeCommit,
    }

    #[derive(Debug)]
    enum HarnessError {
        Sql(anyhow::Error),
        Injected(Failpoint),
    }

    impl std::fmt::Display for HarnessError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Sql(e) => e.fmt(f),
                Self::Injected(fp) => write!(f, "injected failpoint: {fp:?}"),
            }
        }
    }

    impl std::error::Error for HarnessError {}

    impl From<anyhow::Error> for HarnessError {
        fn from(error: anyhow::Error) -> Self {
            // `db.transact` round-trips the closure's error through `anyhow::Error`
            // (the `?` conversions inside `apply_generation` go the other way via
            // the blanket `std::error::Error` impl). Recover the original
            // `HarnessError` — in particular `Injected` — before falling back to
            // wrapping a genuine SQL failure.
            match error.downcast::<HarnessError>() {
                Ok(harness_error) => harness_error,
                Err(error) => Self::Sql(error),
            }
        }
    }

    fn inject(selected: Option<Failpoint>, here: Failpoint) -> Result<(), HarnessError> {
        if selected == Some(here) {
            Err(HarnessError::Injected(here))
        } else {
            Ok(())
        }
    }

    fn configure(db: &Db) -> Result<(), HarnessError> {
        db.execute_batch_on(
            "_pragma",
            "PRAGMA temp_store=FILE; PRAGMA busy_timeout=1000;",
        )?;
        Ok(())
    }

    fn initialize(db: &Db) -> Result<(), HarnessError> {
        configure(db)?;
        db.execute_batch_on(
            "source_state",
            "CREATE TABLE source_state (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE derived_state (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE generation_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO source_state VALUES (1, 'source-old');
             INSERT INTO derived_state VALUES (1, 'derived-old');
             INSERT INTO generation_meta VALUES
                 ('digest', 'digest-old'),
                 ('plan_fingerprint', 'plan-old'),
                 ('watermark', '7');",
        )?;
        Ok(())
    }

    /// Apply all semantic state under exactly one `BEGIN IMMEDIATE` transaction.
    /// `Db::transact` owns the boundary: rollback on any injected or SQL error,
    /// commit on success. `before_commit` is a read-only test seam used to inspect
    /// visibility from a second connection while the writer is still inside the
    /// transaction.
    fn apply_generation(
        db: &Db,
        failpoint: Option<Failpoint>,
        before_commit: impl FnOnce() -> Result<(), HarnessError>,
    ) -> Result<(), HarnessError> {
        db.transact(|| {
            inject(failpoint, Failpoint::BeforeSource)?;

            db.exec_on(
                "source_state",
                "UPDATE source_state SET value = 'source-new' WHERE id = 1",
            )?;
            inject(failpoint, Failpoint::AfterSource)?;

            db.exec_on(
                "derived_state",
                "UPDATE derived_state SET value = 'derived-new' WHERE id = 1",
            )?;
            inject(failpoint, Failpoint::AfterDerived)?;

            db.exec_on(
                "generation_meta",
                "UPDATE generation_meta SET value = 'digest-new' WHERE key = 'digest'",
            )?;
            db.exec_on(
                "generation_meta",
                "UPDATE generation_meta SET value = 'plan-new' WHERE key = 'plan_fingerprint'",
            )?;
            inject(failpoint, Failpoint::AfterDigest)?;

            db.exec_on(
                "generation_meta",
                "UPDATE generation_meta SET value = '8' WHERE key = 'watermark'",
            )?;
            inject(failpoint, Failpoint::BeforeCommit)?;
            before_commit()?;
            Ok(())
        })?;
        Ok(())
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Snapshot {
        source: String,
        derived: String,
        digest: String,
        plan_fingerprint: String,
        watermark: String,
    }

    fn snapshot(db: &Db) -> Result<Snapshot, HarnessError> {
        let scalar = |sql: &str| -> Result<String, HarnessError> {
            Ok(db.query_one("gen_snapshot", sql, &[], |row| Ok(row.get::<_, String>(0)?))?)
        };
        Ok(Snapshot {
            source: scalar("SELECT value FROM source_state WHERE id = 1")?,
            derived: scalar("SELECT value FROM derived_state WHERE id = 1")?,
            digest: scalar("SELECT value FROM generation_meta WHERE key = 'digest'")?,
            plan_fingerprint: scalar(
                "SELECT value FROM generation_meta WHERE key = 'plan_fingerprint'",
            )?,
            watermark: scalar("SELECT value FROM generation_meta WHERE key = 'watermark'")?,
        })
    }

    fn old_snapshot() -> Snapshot {
        Snapshot {
            source: "source-old".into(),
            derived: "derived-old".into(),
            digest: "digest-old".into(),
            plan_fingerprint: "plan-old".into(),
            watermark: "7".into(),
        }
    }

    fn new_snapshot() -> Snapshot {
        Snapshot {
            source: "source-new".into(),
            derived: "derived-new".into(),
            digest: "digest-new".into(),
            plan_fingerprint: "plan-new".into(),
            watermark: "8".into(),
        }
    }

    #[test]
    fn every_failpoint_rolls_back_the_complete_generation() {
        for failpoint in [
            Failpoint::BeforeSource,
            Failpoint::AfterSource,
            Failpoint::AfterDerived,
            Failpoint::AfterDigest,
            Failpoint::BeforeCommit,
        ] {
            let db = db::open(None).unwrap();
            initialize(&db).unwrap();
            let error = apply_generation(&db, Some(failpoint), || Ok(())).unwrap_err();
            assert!(matches!(error, HarnessError::Injected(actual) if actual == failpoint));
            assert_eq!(
                snapshot(&db).unwrap(),
                old_snapshot(),
                "failed at {failpoint:?}"
            );
        }
    }

    #[test]
    fn success_commits_source_derived_digest_plan_and_watermark_together() {
        let db = db::open(None).unwrap();
        initialize(&db).unwrap();
        apply_generation(&db, None, || Ok(())).unwrap();
        assert_eq!(snapshot(&db).unwrap(), new_snapshot());
    }

    static NEXT_DB: AtomicU64 = AtomicU64::new(0);

    fn test_db_path() -> PathBuf {
        let serial = NEXT_DB.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "sprefa-generation-proof-{}-{serial}.sqlite",
            std::process::id(),
        ))
    }

    #[test]
    fn separate_reader_sees_only_committed_generations() {
        let path = test_db_path();
        let writer = db::open(Some(path.to_str().unwrap())).unwrap();
        configure(&writer).unwrap();
        initialize(&writer).unwrap();

        let reader = db::open(Some(path.to_str().unwrap())).unwrap();
        configure(&reader).unwrap();
        reader.begin().unwrap();
        assert_eq!(snapshot(&reader).unwrap(), old_snapshot());

        apply_generation(&writer, None, || {
            assert_eq!(snapshot(&reader)?, old_snapshot());
            Ok(())
        })
        .unwrap();

        // The reader's existing snapshot remains coherent across the writer's
        // commit. A fresh read transaction advances to the new generation.
        assert_eq!(snapshot(&reader).unwrap(), old_snapshot());
        reader.commit().unwrap();
        assert_eq!(snapshot(&reader).unwrap(), new_snapshot());

        drop(reader);
        drop(writer);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }
}
