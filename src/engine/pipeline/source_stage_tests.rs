use super::*;

use crate::db::SqlVal;
use std::time::{SystemTime, UNIX_EPOCH};

fn stage_id(byte: u8) -> StageId {
    StageId::from_bytes([byte; 16])
}
fn base(byte: u8) -> StageBase {
    StageBase::from_bytes([byte; 32])
}

fn file_connection() -> (Db, std::path::PathBuf) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "sprefa-source-stage-{}-{nonce}.db",
        std::process::id()
    ));
    let db = crate::db::open(Some(path.to_str().unwrap())).unwrap();
    db.execute_batch_on("_source_stage", "PRAGMA temp_store=FILE")
        .unwrap();
    (db, path)
}

fn remove_db(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

fn assert_values(actual: &[Value], expected: &[Value]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        match (actual, expected) {
            (Value::Text(a), Value::Text(b)) => assert_eq!(a, b),
            (Value::Int(a), Value::Int(b)) => assert_eq!(a, b),
            (Value::Null, Value::Null) => {}
            pair => panic!("value mismatch: {pair:?}"),
        }
    }
}

#[test]
fn value_codec_is_canonical_roundtrippable_and_rejects_corruption() {
    let values = [Value::Text("x".into()), Value::Int(-2), Value::Null];
    let encoded = encode_values(&values).unwrap();
    let mut golden = b"SPRFVAL\0\0\x01\0\0\0\x03\x01\0\0\0\x01x\x02".to_vec();
    golden.extend_from_slice(&(-2i64).to_be_bytes());
    golden.push(0);
    assert_eq!(encoded, golden);
    assert_eq!(encode_values(&values).unwrap(), encoded);
    assert_values(&decode_values(&encoded).unwrap(), &values);

    let mut bad_version = encoded.clone();
    bad_version[9] = 2;
    let mut bad_tag = encoded.clone();
    bad_tag[14] = 9;
    let mut trailing = encoded.clone();
    trailing.push(0);
    let invalid_utf8 = b"SPRFVAL\0\0\x01\0\0\0\x01\x01\0\0\0\x01\xff".to_vec();
    for corrupt in [
        bad_version,
        bad_tag,
        trailing,
        invalid_utf8,
        encoded[..12].to_vec(),
    ] {
        assert!(matches!(
            decode_values(&corrupt),
            Err(SourceStageError::BadCodec)
        ));
    }
}

#[test]
fn writer_obeys_row_and_encoded_byte_caps() {
    let (db, path) = file_connection();
    let stage = SourceStage::open(&db).unwrap();
    let mut writer = stage
        .begin(
            stage_id(1),
            StageLimits {
                max_rows: 2,
                max_bytes: 1024,
                ..StageLimits::default()
            },
        )
        .unwrap();
    for n in 0..5 {
        writer
            .push("rel", "repo", "a.rs", n as u64, &[Value::Int(n)])
            .unwrap();
    }
    writer.complete_owner("rel", "repo", "a.rs").unwrap();
    let stats = writer.finish().unwrap();
    assert_eq!((stats.flushes, stats.rows, stats.peak_rows), (3, 5, 2));
    assert!(stats.peak_bytes <= 1024);

    let mut writer = stage
        .begin(
            stage_id(2),
            StageLimits {
                max_rows: 100,
                max_bytes: 50,
                ..StageLimits::default()
            },
        )
        .unwrap();
    for n in 0..3 {
        writer
            .push("r", "p", "x", n as u64, &[Value::Int(n)])
            .unwrap();
    }
    writer.complete_owner("r", "p", "x").unwrap();
    let stats = writer.finish().unwrap();
    assert_eq!(
        (stats.flushes, stats.rows, stats.peak_rows, stats.peak_bytes),
        (3, 3, 1, 50)
    );
    drop(db);
    remove_db(&path);
}

#[test]
fn ready_covers_coordinates_counts_digest_and_decoded_rows() {
    let (db, path) = file_connection();
    let stage = SourceStage::open(&db).unwrap();
    let id = stage_id(3);
    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    writer
        .push(
            "fn",
            "repo",
            "src/a.rs",
            0,
            &[Value::Text("a".into()), Value::Int(1)],
        )
        .unwrap();
    writer
        .push("fn", "repo", "src/a.rs", 1, &[Value::Null])
        .unwrap();
    writer
        .push("call", "repo", "src/b.rs", 2, &[Value::Text("b".into())])
        .unwrap();
    writer.complete_owner("fn", "repo", "src/a.rs").unwrap();
    writer.complete_owner("call", "repo", "src/b.rs").unwrap();
    writer.finish().unwrap();
    let ready = stage.seal(id, 7, base(4)).unwrap();
    assert_eq!(
        (ready.generation, ready.key_count, ready.row_count),
        (7, 2, 3)
    );
    assert!(ready.encoded_bytes > 0);
    assert_ne!(ready.digest, [0; 32]);

    let mut seen = Vec::new();
    let count = stage
        .visit_ready_rows(&ready, base(4), |row| {
            seen.push((
                row.relation,
                row.repo,
                row.path,
                row.ordinal,
                row.values.len(),
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 3);
    assert_eq!(
        seen,
        [
            ("call".into(), "repo".into(), "src/b.rs".into(), 2, 1),
            ("fn".into(), "repo".into(), "src/a.rs".into(), 0, 2),
            ("fn".into(), "repo".into(), "src/a.rs".into(), 1, 1),
        ]
    );

    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    writer
        .push(
            "fn",
            "repo",
            "src/a.rs",
            0,
            &[Value::Text("a".into()), Value::Int(1)],
        )
        .unwrap();
    writer
        .push("fn", "repo", "src/a.rs", 1, &[Value::Null])
        .unwrap();
    writer
        .push("call", "repo", "src/b.rs", 2, &[Value::Text("b".into())])
        .unwrap();
    writer.complete_owner("fn", "repo", "src/a.rs").unwrap();
    writer.complete_owner("call", "repo", "src/b.rs").unwrap();
    writer.finish().unwrap();
    let same = stage.seal(id, 7, base(4)).unwrap();
    assert_eq!(same.digest, ready.digest);
    drop(db);
    remove_db(&path);
}

#[test]
fn unsealed_partial_corrupt_and_stale_stages_never_visit_rows() {
    for case in ["unsealed", "partial", "corrupt", "stale-base", "stale-id"] {
        let (db, path) = file_connection();
        let stage = SourceStage::open(&db).unwrap();
        let id = stage_id(5);
        let mut writer = stage.begin(id, StageLimits::default()).unwrap();
        writer
            .push("fn", "repo", "a.rs", 0, &[Value::Int(1)])
            .unwrap();
        writer.complete_owner("fn", "repo", "a.rs").unwrap();
        writer.finish().unwrap();
        let forged = derive_ready(&db, id, 9, base(6)).unwrap();
        let ready = if case == "unsealed" {
            forged.clone()
        } else {
            stage.seal(id, 9, base(6)).unwrap()
        };
        match case {
            "partial" => {
                let encoded = encode_values(&[Value::Int(2)]).unwrap();
                db.exec_params(
                    "_source_stage_row",
                    "INSERT INTO _source_stage_row VALUES (?1,'fn','repo','a.rs',99,?2)",
                    &[SqlVal::Blob(id.0.to_vec()), SqlVal::Blob(encoded)],
                )
                .unwrap();
            }
            "corrupt" => {
                db.exec_params(
                    "_source_stage_row",
                    "UPDATE _source_stage_row SET encoded=x'ff' WHERE stage_id=?1",
                    &[SqlVal::Blob(id.0.to_vec())],
                )
                .unwrap();
            }
            "stale-id" => {
                let mut writer = stage.begin(id, StageLimits::default()).unwrap();
                writer
                    .push("fn", "repo", "a.rs", 0, &[Value::Int(3)])
                    .unwrap();
                writer.complete_owner("fn", "repo", "a.rs").unwrap();
                writer.finish().unwrap();
                stage.seal(id, 10, base(6)).unwrap();
            }
            _ => {}
        }
        let current = if case == "stale-base" {
            base(7)
        } else {
            base(6)
        };
        let mut visited = 0;
        let result = stage.visit_ready_rows(&ready, current, |_| {
            visited += 1;
            Ok(())
        });
        assert!(result.is_err(), "{case} must be refused");
        assert_eq!(visited, 0, "{case} exposed partial rows");
        drop(db);
        remove_db(&path);
    }
}

#[test]
fn owner_completion_is_explicit_and_can_seal_an_exact_empty_owner() {
    let (db, path) = file_connection();
    let stage = SourceStage::open(&db).unwrap();
    let id = stage_id(8);
    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    writer
        .push("fn", "repo", "empty.rs", 0, &[Value::Int(1)])
        .unwrap();
    writer.finish().unwrap();
    assert!(matches!(
        stage.seal(id, 1, base(1)),
        Err(SourceStageError::OwnerIncomplete { .. })
    ));

    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    writer.complete_owner("fn", "repo", "empty.rs").unwrap();
    writer.finish().unwrap();
    let sealed = stage.seal(id, 2, base(1)).unwrap();
    assert_eq!((sealed.key_count, sealed.row_count), (1, 0));
    assert_eq!(
        stage
            .visit_ready_rows(&sealed, base(1), |_| unreachable!())
            .unwrap(),
        0
    );
    stage.discard(id).unwrap();
    assert!(matches!(
        stage.visit_ready_rows(&sealed, base(1), |_| Ok(())),
        Err(SourceStageError::Unsealed)
    ));
    drop(db);
    remove_db(&path);
}

#[test]
fn stage_tables_are_file_backed_temp_and_connection_local() {
    let (db, path) = file_connection();
    let _stage = SourceStage::open(&db).unwrap();
    let temp_store: i64 = db
        .query_one("_pragma", "PRAGMA temp_store", &[], |row| Ok(row.get(0)?))
        .unwrap();
    let tables: i64 = db
        .query_one(
            "sqlite_temp_master",
            "SELECT count(*) FROM sqlite_temp_master WHERE name LIKE '_source_stage_%'",
            &[],
            |row| Ok(row.get(0)?),
        )
        .unwrap();
    assert_eq!((temp_store, tables), (1, 3));
    let other = crate::db::open(Some(path.to_str().unwrap())).unwrap();
    assert!(other
        .query_rows::<()>("_source_stage_row", "SELECT * FROM _source_stage_row", &[], |_| {
            Ok(())
        })
        .is_err());
    drop(other);
    drop(db);
    remove_db(&path);
}

#[test]
fn ready_row_pages_obey_encoded_byte_budget() {
    let (db, path) = file_connection();
    let stage = SourceStage::open(&db).unwrap();
    let id = stage_id(9);
    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    for ordinal in 0..3 {
        writer
            .push(
                "facts",
                "repo",
                "large.rs",
                ordinal,
                &[Value::Text("x".repeat(64 * 1024))],
            )
            .unwrap();
    }
    writer.complete_owner("facts", "repo", "large.rs").unwrap();
    writer.finish().unwrap();
    let ready = stage.seal(id, 1, base(1)).unwrap();
    let first = stage
        .read_ready_rows_after(&ready, None, 4096, 128 * 1024)
        .unwrap();
    assert_eq!(first.len(), 1);
    let cursor = read::SourceRowCursor::from_row(first.last().unwrap());
    let second = stage
        .read_ready_rows_after(&ready, Some(&cursor), 4096, 128 * 1024)
        .unwrap();
    assert_eq!(second.len(), 1);
    drop(db);
    remove_db(&path);
}

/// Fail-pre-fix N+1 pin: 200 owner completions (well past db.rs's
/// N1_THRESHOLD of 64) must land as chunked inserts at flush time, not one
/// INSERT per `complete_owner` call. Pre-fix `tick_end` screamed
/// `('_source_stage_owner', 201)`; post-fix the whole stage stays under the
/// threshold for every statement key.
#[test]
fn owner_completions_batch_and_never_trip_the_n1_scream() {
    let (db, path) = file_connection();
    let stage = SourceStage::open(&db).unwrap();
    let id = stage_id(11);
    let mut writer = stage.begin(id, StageLimits::default()).unwrap();
    db.tick_begin();
    for i in 0..200 {
        let owner_path = format!("src/f{i}.rs");
        writer
            .push("fn", "repo", &owner_path, 0, &[Value::Int(1)])
            .unwrap();
        writer.complete_owner("fn", "repo", &owner_path).unwrap();
    }
    writer.finish().unwrap();
    assert_eq!(
        db.tick_end(),
        None,
        "owner completions must land batched, not one statement per call"
    );
    // The batched path lands the same owner set the per-call path did.
    let owners: i64 = db
        .query_one(
            "_source_stage_owner",
            "SELECT COUNT(*) FROM _source_stage_owner WHERE stage_id=?1",
            &[SqlVal::Blob(id.0.to_vec())],
            |row| Ok(row.get(0)?),
        )
        .unwrap();
    assert_eq!(owners, 200);
    drop(db);
    remove_db(&path);
}
