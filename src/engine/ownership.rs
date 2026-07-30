//! Slice A executable ownership contract.
//!
//! This module is test-only until Slice B routes extraction through the shadow
//! tables. Keeping the representative `df_node` schema and SQL plans executable
//! prevents the design contract from drifting while production remains on the
//! legacy refresh path.

#[cfg(test)]
mod tests {
    use crate::db::{self, Db, SqlVal};

    const SHADOW_DDL: &str = r#"
        PRAGMA foreign_keys=ON;
        PRAGMA temp_store=FILE;

        CREATE TABLE _repo_identity_v1 (
            repo_id BLOB PRIMARY KEY CHECK(length(repo_id)=16),
            slug TEXT NOT NULL,
            root TEXT NOT NULL UNIQUE,
            url TEXT NOT NULL DEFAULT ''
        ) WITHOUT ROWID;

        CREATE TABLE _family_schema_v1 (
            family TEXT PRIMARY KEY,
            schema_epoch INTEGER NOT NULL,
            extractor_schema INTEGER NOT NULL,
            state TEXT NOT NULL CHECK(state IN
              ('shadow','validating','active','retired','failed')),
            writer_min_version INTEGER NOT NULL,
            active_suffix TEXT NOT NULL,
            migration_id BLOB CHECK(migration_id IS NULL OR length(migration_id)=16),
            activated_generation INTEGER,
            updated_at_ms INTEGER NOT NULL,
            error TEXT
        ) WITHOUT ROWID;

        CREATE TABLE _family_migration_v1 (
            migration_id BLOB PRIMARY KEY CHECK(length(migration_id)=16),
            family TEXT NOT NULL REFERENCES _family_schema_v1(family),
            from_epoch INTEGER NOT NULL,
            to_epoch INTEGER NOT NULL,
            state TEXT NOT NULL CHECK(state IN
              ('building','validating','ready','active','cleanup','failed')),
            resume_repo_id BLOB,
            resume_coordinate_id BLOB,
            resume_path TEXT,
            owners_done INTEGER NOT NULL DEFAULT 0,
            facts_done INTEGER NOT NULL DEFAULT 0,
            bytes_done INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            error TEXT
        ) WITHOUT ROWID;
        CREATE INDEX _family_migration_state_v1
          ON _family_migration_v1(state,updated_at_ms);

        CREATE TABLE _root_fence_v1 (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            fence_token INTEGER NOT NULL,
            generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
            claim_through_seq INTEGER NOT NULL,
            installed_at_ms INTEGER NOT NULL
        );

        CREATE TABLE _root_generation_v1 (
            singleton INTEGER PRIMARY KEY CHECK(singleton=1),
            committed_generation INTEGER NOT NULL,
            generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
            scheduler_claim_seq INTEGER NOT NULL,
            fence_token INTEGER NOT NULL,
            program_digest BLOB NOT NULL CHECK(length(program_digest)=32),
            committed_at_ms INTEGER NOT NULL
        );

        CREATE TABLE _df_owner_v1 (
            owner_id BLOB PRIMARY KEY CHECK(length(owner_id)=16),
            repo_id BLOB NOT NULL CHECK(length(repo_id)=16)
              REFERENCES _repo_identity_v1(repo_id),
            coordinate_id BLOB NOT NULL CHECK(length(coordinate_id)=32),
            normalized_path TEXT NOT NULL CHECK(instr(normalized_path,char(0))=0),
            extractor_schema INTEGER NOT NULL,
            content_id BLOB NOT NULL CHECK(length(content_id)=32),
            committed_generation INTEGER NOT NULL,
            UNIQUE(repo_id,coordinate_id,normalized_path,extractor_schema)
        ) WITHOUT ROWID;
        CREATE INDEX _df_owner_generation_v1
          ON _df_owner_v1(committed_generation);

        CREATE TABLE _fact_df_node_v1 (
            fact_id BLOB PRIMARY KEY CHECK(length(fact_id)=16),
            owner_count INTEGER NOT NULL CHECK(owner_count>=0),
            id INTEGER NOT NULL UNIQUE,
            kind INTEGER NOT NULL,
            var INTEGER NOT NULL,
            fn INTEGER NOT NULL,
            file INTEGER NOT NULL,
            line INTEGER NOT NULL
        ) WITHOUT ROWID;

        CREATE TABLE _own_df_node_v1 (
            owner_id BLOB NOT NULL CHECK(length(owner_id)=16)
              REFERENCES _df_owner_v1(owner_id),
            fact_id BLOB NOT NULL CHECK(length(fact_id)=16)
              REFERENCES _fact_df_node_v1(fact_id),
            PRIMARY KEY(owner_id,fact_id)
        ) WITHOUT ROWID;
        CREATE INDEX _own_df_node_by_fact_v1
          ON _own_df_node_v1(fact_id,owner_id);

        CREATE TABLE _df_stage_owner_v1 (
            generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
            owner_id BLOB NOT NULL CHECK(length(owner_id)=16),
            operation INTEGER NOT NULL CHECK(operation IN (1,2)),
            repo_id BLOB NOT NULL CHECK(length(repo_id)=16),
            coordinate_id BLOB NOT NULL CHECK(length(coordinate_id)=32),
            normalized_path TEXT NOT NULL,
            extractor_schema INTEGER NOT NULL,
            content_id BLOB CHECK(content_id IS NULL OR length(content_id)=32),
            program_digest BLOB NOT NULL CHECK(length(program_digest)=32),
            fence_token INTEGER NOT NULL,
            staged_complete INTEGER NOT NULL DEFAULT 0
              CHECK(staged_complete IN (0,1)),
            PRIMARY KEY(generation_id,owner_id)
        ) WITHOUT ROWID;
        CREATE INDEX _df_stage_owner_ready_v1
          ON _df_stage_owner_v1(generation_id,staged_complete,owner_id);

        CREATE TABLE _stage_df_node_v1 (
            generation_id BLOB NOT NULL CHECK(length(generation_id)=16),
            owner_id BLOB NOT NULL CHECK(length(owner_id)=16),
            fact_id BLOB NOT NULL CHECK(length(fact_id)=16),
            id INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            var INTEGER NOT NULL,
            fn INTEGER NOT NULL,
            file INTEGER NOT NULL,
            line INTEGER NOT NULL,
            PRIMARY KEY(generation_id,owner_id,fact_id)
        ) WITHOUT ROWID;
        CREATE INDEX _stage_df_node_fact_v1
          ON _stage_df_node_v1(generation_id,fact_id,owner_id);

        CREATE TEMP TABLE tx_changed_owner (
            owner_id BLOB PRIMARY KEY CHECK(length(owner_id)=16),
            operation INTEGER NOT NULL CHECK(operation IN (1,2))
        ) WITHOUT ROWID;
        CREATE TEMP TABLE tx_edge_change (
            owner_id BLOB NOT NULL CHECK(length(owner_id)=16),
            fact_id BLOB NOT NULL CHECK(length(fact_id)=16),
            diff INTEGER NOT NULL CHECK(diff IN (-1,1)),
            PRIMARY KEY(owner_id,fact_id)
        ) WITHOUT ROWID;
        CREATE INDEX tx_edge_change_fact
          ON tx_edge_change(fact_id,owner_id,diff);
        CREATE TEMP TABLE tx_fact_net (
            fact_id BLOB PRIMARY KEY CHECK(length(fact_id)=16),
            net INTEGER NOT NULL
        ) WITHOUT ROWID;
        CREATE TEMP TABLE tx_fact_apply_df_node (
            fact_id BLOB PRIMARY KEY CHECK(length(fact_id)=16),
            old_count INTEGER NOT NULL CHECK(old_count>=0),
            new_count INTEGER NOT NULL CHECK(new_count>=0),
            id INTEGER NOT NULL,
            kind INTEGER NOT NULL,
            var INTEGER NOT NULL,
            fn INTEGER NOT NULL,
            file INTEGER NOT NULL,
            line INTEGER NOT NULL
        ) WITHOUT ROWID;
    "#;

    const REMOVED_PLAN: &str = r#"
        SELECT e.owner_id,e.fact_id
        FROM tx_changed_owner AS c
        CROSS JOIN _own_df_node_v1 AS e ON e.owner_id=c.owner_id
        LEFT JOIN _stage_df_node_v1 AS s
          ON s.generation_id=?1
         AND s.owner_id=e.owner_id
         AND s.fact_id=e.fact_id
        WHERE c.operation IN (1,2) AND s.fact_id IS NULL
    "#;

    const ADDED_PLAN: &str = r#"
        SELECT s.owner_id,s.fact_id
        FROM tx_changed_owner AS c
        CROSS JOIN _stage_df_node_v1 AS s
          ON s.generation_id=?1 AND s.owner_id=c.owner_id
        LEFT JOIN _own_df_node_v1 AS e
          ON e.owner_id=s.owner_id AND e.fact_id=s.fact_id
        WHERE c.operation=1 AND e.fact_id IS NULL
    "#;

    fn contract_db() -> Db {
        let db = db::open(None).unwrap();
        db.execute_batch_on("_repo_identity_v1", SHADOW_DDL)
            .unwrap();
        db
    }

    fn explain(db: &Db, sql: &str) -> Vec<String> {
        db.query_rows(
            "_explain_plan",
            &format!("EXPLAIN QUERY PLAN {sql}"),
            &[SqlVal::from(vec![0x44_u8; 16])],
            |row| Ok(row.get::<_, String>(3)?),
        )
        .unwrap()
    }

    fn assert_delta_plan_is_bounded(plan: &[String]) {
        let text = plan.join("\n");
        for forbidden in [
            "SCAN _own_",
            "SCAN _fact_",
            "SUBQUERY",
            "EXCEPT",
            "USE TEMP B-TREE",
        ] {
            assert!(
                !text.contains(forbidden),
                "forbidden plan shape {forbidden:?}:\n{text}"
            );
        }
        assert!(
            text.contains("SEARCH e USING PRIMARY KEY"),
            "ownership PK probe absent:\n{text}"
        );
        assert!(
            text.contains("SEARCH s USING PRIMARY KEY"),
            "staging PK probe absent:\n{text}"
        );
    }

    fn insert_fact(
        db: &Db,
        fact_id: &[u8; 16],
        id: i64,
        kind: i64,
        owner_count: i64,
    ) -> anyhow::Result<usize> {
        db.exec_params(
            "_fact_df_node_v1",
            "INSERT INTO _fact_df_node_v1
             (fact_id,owner_count,id,kind,var,fn,file,line)
             VALUES (?1,?2,?3,?4,12,13,14,15)",
            &[
                SqlVal::from(fact_id.as_slice()),
                SqlVal::from(owner_count),
                SqlVal::from(id),
                SqlVal::from(kind),
            ],
        )
    }

    #[test]
    fn shadow_contract_uses_foreign_keys_and_file_temp() {
        let db = contract_db();
        let foreign_keys = db.pragma_i64("foreign_keys").unwrap();
        let temp_store = db.pragma_i64("temp_store").unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(temp_store, 1);
        for table in [
            "_repo_identity_v1",
            "_family_schema_v1",
            "_family_migration_v1",
            "_df_owner_v1",
            "_fact_df_node_v1",
            "_own_df_node_v1",
            "_df_stage_owner_v1",
            "_stage_df_node_v1",
        ] {
            let found: i64 = db
                .query_one(
                    "sqlite_master",
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    &[SqlVal::from(table)],
                    |r| Ok(r.get(0)?),
                )
                .unwrap();
            assert_eq!(found, 1, "missing contract table {table}");
        }
    }

    #[test]
    fn fact_id_and_semantic_key_conflicts_roll_back_atomically() {
        let db = contract_db();
        let fact_a = [0x11; 16];
        let fact_b = [0x22; 16];

        db.begin().unwrap();
        insert_fact(&db, &fact_a, 7, 8, 0).unwrap();
        let collision = insert_fact(&db, &fact_a, 7, 9, 0).unwrap_err();
        assert!(collision.to_string().contains("UNIQUE constraint failed"));
        db.rollback().unwrap();
        let count: i64 = db
            .query_one(
                "_fact_df_node_v1",
                "SELECT COUNT(*) FROM _fact_df_node_v1",
                &[],
                |r| Ok(r.get(0)?),
            )
            .unwrap();
        assert_eq!(count, 0, "FactId collision must roll back the generation");

        db.begin().unwrap();
        insert_fact(&db, &fact_a, 7, 8, 0).unwrap();
        let semantic = insert_fact(&db, &fact_b, 7, 8, 0).unwrap_err();
        assert!(semantic.to_string().contains("UNIQUE constraint failed"));
        db.rollback().unwrap();
        let count: i64 = db
            .query_one(
                "_fact_df_node_v1",
                "SELECT COUNT(*) FROM _fact_df_node_v1",
                &[],
                |r| Ok(r.get(0)?),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "semantic-key conflict must roll back the generation"
        );
    }

    #[test]
    fn flat_delta_plans_probe_persistent_indexes() {
        let db = contract_db();
        let removed = explain(&db, REMOVED_PLAN);
        let added = explain(&db, ADDED_PLAN);
        // Guard the bounded-plan check against a vacuous pass: an empty plan
        // trivially satisfies "no full scan", so prove EXPLAIN actually
        // produced plan rows before trusting the shape assertions below.
        assert!(!removed.is_empty(), "REMOVED_PLAN produced no query plan");
        assert!(!added.is_empty(), "ADDED_PLAN produced no query plan");
        assert_delta_plan_is_bounded(&removed);
        assert_delta_plan_is_bounded(&added);
    }

    #[test]
    fn flat_bulk_upsert_applies_owner_counts() {
        let db = contract_db();
        let fact_id = [0x33; 16];
        insert_fact(&db, &fact_id, 21, 22, 1).unwrap();
        db.exec_params(
            "tx_fact_apply_df_node",
            "INSERT INTO tx_fact_apply_df_node
             (fact_id,old_count,new_count,id,kind,var,fn,file,line)
             VALUES (?1,1,3,21,22,12,13,14,15)",
            &[SqlVal::from(fact_id.as_slice())],
        )
        .unwrap();
        db.execute_batch_on(
            "_fact_df_node_v1",
            "INSERT INTO _fact_df_node_v1
               (fact_id,owner_count,id,kind,var,fn,file,line)
             SELECT fact_id,new_count,id,kind,var,fn,file,line
             FROM tx_fact_apply_df_node
             WHERE true
             ON CONFLICT(fact_id) DO UPDATE SET
               owner_count=excluded.owner_count;",
        )
        .unwrap();
        let count: i64 = db
            .query_one(
                "_fact_df_node_v1",
                "SELECT owner_count FROM _fact_df_node_v1 WHERE fact_id=?1",
                &[SqlVal::from(fact_id.as_slice())],
                |r| Ok(r.get(0)?),
            )
            .unwrap();
        assert_eq!(count, 3);
    }
}
