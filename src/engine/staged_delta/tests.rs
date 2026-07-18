use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn sealed_exact_stage_commits_manifest_and_watermark() {
    let mut h = StagedDeltaHarness::new().unwrap();
    h.seed(&[
        FixtureRow::new("r", "a", 1),
        FixtureRow::new("r", "a", 2),
        FixtureRow::new("r", "b", 9),
    ])
    .unwrap();
    let (ready, sink) = h
        .stage_generation(
            1,
            &[ChangedKey::new("r", "a")],
            [FixtureRow::new("r", "a", 2), FixtureRow::new("r", "a", 3)],
            SinkLimits::default(),
        )
        .unwrap();
    let report = h.consume_stage(&ready, sink, FailPoint::None).unwrap();
    assert_eq!((report.plus, report.minus), (1, 1));
    assert_eq!(
        h.rows().unwrap(),
        [
            FixtureRow::new("r", "a", 2),
            FixtureRow::new("r", "a", 3),
            FixtureRow::new("r", "b", 9)
        ]
    );
    assert_eq!(
        h.manifest(1).unwrap(),
        Some(Manifest {
            key_count: 1,
            row_count: 2,
            plus: 1,
            minus: 1,
            digest: ready.digest,
            exact: true,
        })
    );
    assert_eq!(h.watermark().unwrap(), Some((1, ready.digest)));
}

#[test]
fn unsealed_aborted_and_partial_stages_are_refused_without_changes() {
    let cases = ["unsealed", "aborted", "partial"];
    for (offset, case) in cases.into_iter().enumerate() {
        let mut h = StagedDeltaHarness::new().unwrap();
        h.seed(&[FixtureRow::new("r", "a", 1)]).unwrap();
        let generation = 10 + offset as i64;
        let sink = h
            .stage_unsealed(
                &[ChangedKey::new("r", "a")],
                [FixtureRow::new("r", "a", 2)],
                SinkLimits::default(),
            )
            .unwrap();
        let ready = derive_stage_ready(&h.db, generation).unwrap();
        match case {
            "unsealed" => {}
            "aborted" => {
                let sealed = h.seal_stage(generation).unwrap();
                assert_eq!(sealed, ready);
                h.abort_stage().unwrap();
            }
            "partial" => {
                let sealed = h.seal_stage(generation).unwrap();
                assert_eq!(sealed, ready);
                h.db
                    .exec_on("_next", "INSERT INTO _next(scope, name, value) VALUES ('r', 'a', 3)")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let error = h.consume_stage(&ready, sink, FailPoint::None).unwrap_err();
        assert!(matches!(
            error,
            StageError::StageNotReady | StageError::StageSealMismatch
        ));
        assert_eq!(h.rows().unwrap(), [FixtureRow::new("r", "a", 1)]);
        assert_eq!(h.manifest(generation).unwrap(), None);
        assert_eq!(h.watermark().unwrap(), None);
    }
}

#[test]
fn every_consume_failpoint_rolls_back_rows_manifest_and_watermark() {
    let points = [
        FailPoint::AfterPlus,
        FailPoint::AfterMinus,
        FailPoint::AfterDelete,
        FailPoint::AfterInsert,
        FailPoint::AfterManifestWatermark,
    ];
    for (offset, fail) in points.into_iter().enumerate() {
        let mut h = StagedDeltaHarness::new().unwrap();
        h.seed(&[FixtureRow::new("r", "a", 1)]).unwrap();
        let generation = 20 + offset as i64;
        let error = h
            .apply_generation(
                generation,
                &[ChangedKey::new("r", "a")],
                [FixtureRow::new("r", "a", 2)],
                SinkLimits::default(),
                fail,
            )
            .unwrap_err();
        assert!(matches!(error, StageError::Injected(point) if point == fail));
        assert_eq!(h.rows().unwrap(), [FixtureRow::new("r", "a", 1)]);
        assert_eq!(h.manifest(generation).unwrap(), None);
        assert_eq!(h.watermark().unwrap(), None);
    }
}

#[test]
fn duplicate_set_parity_and_bounded_sink_caps() {
    let mut h = StagedDeltaHarness::new().unwrap();
    h.seed(&[FixtureRow::new("r", "a", 1), FixtureRow::new("r", "a", 1)])
        .unwrap();
    let report = h
        .apply_generation(
            30,
            &[ChangedKey::new("r", "a"), ChangedKey::new("r", "a")],
            [FixtureRow::new("r", "a", 1), FixtureRow::new("r", "a", 1)],
            SinkLimits {
                max_rows: 1,
                max_bytes: 64,
            },
            FailPoint::None,
        )
        .unwrap();
    assert_eq!((report.plus, report.minus), (0, 0));
    assert_eq!(report.sink.flushes, 2);
    assert_eq!(report.sink.peak_rows, 1);
    assert!(report.sink.peak_bytes <= 64);
    assert_eq!(h.rows().unwrap(), [FixtureRow::new("r", "a", 1)]);
}

#[test]
fn populated_analyzed_plans_use_changed_key_driver_and_pk_probes() {
    let h = StagedDeltaHarness::new().unwrap();
    let rows: Vec<_> = (0..512)
        .map(|n| FixtureRow::new(&format!("repo{}", n % 8), &format!("name{}", n % 32), n))
        .collect();
    h.seed(&rows).unwrap();
    h.stage_unsealed(
        &[
            ChangedKey::new("repo1", "name1"),
            ChangedKey::new("repo2", "name2"),
        ],
        [
            FixtureRow::new("repo1", "name1", 900),
            FixtureRow::new("repo2", "name2", 901),
        ],
        SinkLimits::default(),
    )
    .unwrap();
    h.db
        .execute_batch_on("rel_fixture", "ANALYZE rel_fixture; ANALYZE _next; ANALYZE _changed_key;")
        .unwrap();
    assert_diff_plans(&h.db).unwrap();
    let auto_index = h.db.pragma_i64("automatic_index").unwrap();
    let temp_store = h.db.pragma_i64("temp_store").unwrap();
    assert_eq!((auto_index, temp_store), (0, 1));
}

#[test]
fn null_identity_is_refused_by_every_staging_table() {
    let h = StagedDeltaHarness::new().unwrap();
    for sql in [
        "INSERT INTO rel_fixture(scope,name,value) VALUES(NULL,'a',1)",
        "INSERT INTO _next(scope,name,value) VALUES('r',NULL,1)",
        "INSERT INTO _changed_key(scope,name) VALUES('r',NULL)",
        "INSERT INTO _plus(scope,name,value) VALUES('r','a',NULL)",
        "INSERT INTO _minus(scope,name,value) VALUES(NULL,'a',1)",
    ] {
        let error = h.db.exec_on("_null_check", sql).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("NOT NULL constraint failed"),
            "expected a NOT NULL constraint failure, got: {message}"
        );
    }
}

#[test]
fn wal_reader_sees_atomic_snapshot_then_new_generation_after_restart() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("sprefa-stage-{}-{nonce}.db", std::process::id()));
    let mut writer = StagedDeltaHarness::open_file(&path).unwrap();
    writer.seed(&[FixtureRow::new("r", "a", 1)]).unwrap();

    // A second, independent read-only connection on the same WAL file — the
    // point of this test is cross-connection snapshot isolation, so it
    // deliberately reads outside the `Db` seam via `db::open_read_only`
    // (raw connection, manual BEGIN/COMMIT) rather than through `writer`.
    let path_str = path.to_str().unwrap();
    let reader = crate::db::open_read_only(path_str).unwrap();
    let read_reader_rows = |conn: &rusqlite::Connection| -> Vec<FixtureRow> {
        let mut stmt = conn
            .prepare("SELECT scope, name, value FROM rel_fixture ORDER BY scope, name, value")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(FixtureRow {
                scope: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };

    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(read_reader_rows(&reader), [FixtureRow::new("r", "a", 1)]);

    writer
        .apply_generation(
            40,
            &[ChangedKey::new("r", "a")],
            [FixtureRow::new("r", "a", 2)],
            SinkLimits::default(),
            FailPoint::None,
        )
        .unwrap();
    assert_eq!(read_reader_rows(&reader), [FixtureRow::new("r", "a", 1)]);
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(read_reader_rows(&reader), [FixtureRow::new("r", "a", 2)]);

    drop(reader);
    drop(writer);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
