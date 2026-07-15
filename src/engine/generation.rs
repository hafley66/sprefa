//! Isolated proof that one semantic generation can commit atomically.
//!
//! This module is intentionally not wired into `engine` yet.  It models the
//! transaction boundary required by source deltas and derived deltaflow before
//! production orchestration adopts it.

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, TransactionBehavior};
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
        Sql(rusqlite::Error),
        Injected(Failpoint),
    }

    impl From<rusqlite::Error> for HarnessError {
        fn from(error: rusqlite::Error) -> Self {
            Self::Sql(error)
        }
    }

    type Result<T> = std::result::Result<T, HarnessError>;

    fn inject(selected: Option<Failpoint>, here: Failpoint) -> Result<()> {
        if selected == Some(here) {
            Err(HarnessError::Injected(here))
        } else {
            Ok(())
        }
    }

    fn configure(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch("PRAGMA temp_store=FILE; PRAGMA busy_timeout=1000;")
    }

    fn initialize(conn: &Connection) -> rusqlite::Result<()> {
        configure(conn)?;
        conn.execute_batch(
            "CREATE TABLE source_state (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE derived_state (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE generation_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO source_state VALUES (1, 'source-old');
             INSERT INTO derived_state VALUES (1, 'derived-old');
             INSERT INTO generation_meta VALUES
                 ('digest', 'digest-old'),
                 ('plan_fingerprint', 'plan-old'),
                 ('watermark', '7');",
        )
    }

    /// Apply all semantic state under exactly one `BEGIN IMMEDIATE` transaction.
    /// Dropping `tx` on any injected or SQL error proves the rollback behavior
    /// production wiring must preserve. `before_commit` is a read-only test seam
    /// used to inspect visibility from a second connection while the writer is
    /// still inside the transaction.
    fn apply_generation(
        conn: &mut Connection,
        failpoint: Option<Failpoint>,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        inject(failpoint, Failpoint::BeforeSource)?;

        tx.execute(
            "UPDATE source_state SET value = 'source-new' WHERE id = 1",
            [],
        )?;
        inject(failpoint, Failpoint::AfterSource)?;

        tx.execute(
            "UPDATE derived_state SET value = 'derived-new' WHERE id = 1",
            [],
        )?;
        inject(failpoint, Failpoint::AfterDerived)?;

        tx.execute(
            "UPDATE generation_meta SET value = 'digest-new' WHERE key = 'digest'",
            [],
        )?;
        tx.execute(
            "UPDATE generation_meta SET value = 'plan-new' WHERE key = 'plan_fingerprint'",
            [],
        )?;
        inject(failpoint, Failpoint::AfterDigest)?;

        tx.execute(
            "UPDATE generation_meta SET value = '8' WHERE key = 'watermark'",
            [],
        )?;
        inject(failpoint, Failpoint::BeforeCommit)?;
        before_commit()?;
        tx.commit()?;
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

    fn snapshot(conn: &Connection) -> rusqlite::Result<Snapshot> {
        let scalar = |sql: &str| conn.query_row(sql, [], |row| row.get::<_, String>(0));
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
            let mut conn = Connection::open_in_memory().unwrap();
            initialize(&conn).unwrap();
            let error = apply_generation(&mut conn, Some(failpoint), || Ok(())).unwrap_err();
            assert!(matches!(error, HarnessError::Injected(actual) if actual == failpoint));
            assert_eq!(
                snapshot(&conn).unwrap(),
                old_snapshot(),
                "failed at {failpoint:?}"
            );
        }
    }

    #[test]
    fn success_commits_source_derived_digest_plan_and_watermark_together() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        apply_generation(&mut conn, None, || Ok(())).unwrap();
        assert_eq!(snapshot(&conn).unwrap(), new_snapshot());
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
        let mut writer = Connection::open(&path).unwrap();
        configure(&writer).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        initialize(&writer).unwrap();

        let reader = Connection::open(&path).unwrap();
        configure(&reader).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        assert_eq!(snapshot(&reader).unwrap(), old_snapshot());

        apply_generation(&mut writer, None, || {
            assert_eq!(snapshot(&reader)?, old_snapshot());
            Ok(())
        })
        .unwrap();

        // The reader's existing snapshot remains coherent across the writer's
        // commit. A fresh read transaction advances to the new generation.
        assert_eq!(snapshot(&reader).unwrap(), old_snapshot());
        reader.execute_batch("COMMIT").unwrap();
        assert_eq!(snapshot(&reader).unwrap(), new_snapshot());

        drop(reader);
        drop(writer);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }
}
