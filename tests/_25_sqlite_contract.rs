//! The `open()` contract and the `sql` span under load: the eight rows of the
//! eval-on-sqlite-ivm addendum. Every expectation is a COUNT, a ratio against
//! a formula, or the one named wall clock (batched 10k under 50 ms). A file db
//! under the test's temp dir, every connection through `open()`, every
//! statement through `sql()`.

use dl8::_9_runtime::{open, sql};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;
use tracing_subscriber::layer::SubscriberExt;

/// One closed span: its kind (`sql` or `phase`), its `name` field, the
/// recorded `rows` and `ms`, and the wall time between open and close.
#[derive(Clone)]
struct RecordedSpan {
    kind: String,
    name: Option<String>,
    rows: Option<i64>,
    ms: Option<f64>,
    started: Instant,
    elapsed_ms: Option<f64>,
}

impl tracing::field::Visit for RecordedSpan {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.name = Some(value.to_string());
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if field.name() == "rows" {
            self.rows = Some(value);
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_i64(field, value as i64);
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if field.name() == "ms" {
            self.ms = Some(value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.name = Some(format!("{value:?}"));
        }
    }
}

/// The installed layer. Closed spans go down one channel.
#[derive(Clone)]
struct Collector(mpsc::Sender<RecordedSpan>);

impl<S> tracing_subscriber::Layer<S> for Collector
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut recorded = RecordedSpan {
            kind: span.name().to_string(),
            name: None,
            rows: None,
            ms: None,
            started: Instant::now(),
            elapsed_ms: None,
        };
        attrs.record(&mut recorded);
        span.extensions_mut().insert(recorded);
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(recorded) = extensions.get_mut::<RecordedSpan>() {
            values.record(&mut *recorded);
        }
    }

    fn on_close(&self, id: tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(recorded) = extensions.get_mut::<RecordedSpan>() {
            recorded.elapsed_ms = Some(recorded.started.elapsed().as_secs_f64() * 1000.0);
            let _ = self.0.send(recorded.clone());
        }
    }
}

/// Runs `body` under the collector and returns every closed span.
fn traced<T>(body: impl FnOnce() -> T) -> Vec<RecordedSpan> {
    let (sender, receiver) = mpsc::channel();
    let guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(Collector(sender)));
    let _ = body();
    drop(guard);
    receiver.try_iter().collect()
}

fn span_ms(spans: &[RecordedSpan], kind: &str, name: &str) -> f64 {
    spans
        .iter()
        .filter(|span| span.kind == kind && span.name.as_deref() == Some(name))
        .map(|span| span.ms.unwrap_or(0.0))
        .sum()
}

fn span_count(spans: &[RecordedSpan], kind: &str, name: &str) -> usize {
    spans
        .iter()
        .filter(|span| span.kind == kind && span.name.as_deref() == Some(name))
        .count()
}

/// RSS peak of this process, bytes (`getrusage` reports KiB off macOS).
fn max_rss_bytes() -> i64 {
    #[repr(C)]
    struct Rusage {
        utime: [i64; 2],
        stime: [i64; 2],
        maxrss: i64,
        rest: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage = std::mem::MaybeUninit::<Rusage>::zeroed();
    let code = unsafe { getrusage(0, usage.as_mut_ptr()) };
    assert_eq!(code, 0, "getrusage failed");
    let usage = unsafe { usage.assume_init() };
    if cfg!(target_os = "macos") {
        usage.maxrss
    } else {
        usage.maxrss * 1024
    }
}

fn directory(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dl8-sqlite-contract-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The tests measure RSS peaks, so they never overlap each other.
fn lane() -> std::sync::MutexGuard<'static, ()> {
    static LANE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LANE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn declare(connection: &Connection, ddl: &str) {
    sql(connection, "declare", |connection| {
        connection.execute_batch(ddl).map(|()| ((), 0))
    })
    .unwrap();
}

fn begin(connection: &Connection) {
    sql(connection, "begin", |connection| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map(|()| ((), 0))
    })
    .unwrap();
}

fn commit(connection: &Connection) {
    sql(connection, "commit", |connection| {
        connection.execute_batch("COMMIT").map(|()| ((), 0))
    })
    .unwrap();
}

/// One multi-row INSERT, one `sql` span with its row count.
fn insert_values(connection: &Connection, table: &str, values: &[i64], name: &'static str) {
    sql(connection, name, |connection| {
        let tuples = vec!["(?)"; values.len()].join(",");
        let inserted = connection.execute(
            &format!("INSERT INTO \"{table}\" (\"n\") VALUES {tuples}"),
            rusqlite::params_from_iter(values.iter()),
        )?;
        Ok((inserted, inserted))
    })
    .unwrap();
}

/// One multi-row INSERT of `(left, right)` pairs, one `sql` span with its row
/// count.
fn insert_pairs(connection: &Connection, table: &str, columns: &str, pairs: &[(i64, i64)]) {
    sql(connection, "insert_rows", |connection| {
        let tuples = vec!["(?, ?)"; pairs.len()].join(",");
        let mut values = Vec::with_capacity(pairs.len() * 2);
        for (left, right) in pairs {
            values.push(*left);
            values.push(*right);
        }
        let inserted = connection.execute(
            &format!("INSERT INTO \"{table}\" ({columns}) VALUES {tuples}"),
            rusqlite::params_from_iter(values.iter()),
        )?;
        Ok((inserted, inserted))
    })
    .unwrap();
}

#[test]
fn per_statement_insert_costs_twenty_times_one_batched_insert() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("n_plus_one");

    let statement_spans = traced(|| {
        let connection = open(&directory.join("per_statement.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        for n in 0..rows {
            insert_values(&connection, "t", &[n], "insert_row");
        }
    });

    let batched_spans = traced(|| {
        let connection = open(&directory.join("batched.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        let values: Vec<i64> = (0..rows).collect();
        insert_values(&connection, "t", &values, "insert_rows");
        commit(&connection);
    });

    let per_statement_ms = span_ms(&statement_spans, "sql", "insert_row");
    let batched_ms = span_ms(&batched_spans, "sql", "insert_rows");
    println!(
        "per-statement {per_statement_ms:.1} ms over 10000 spans, batched {batched_ms:.1} ms over 1 span"
    );

    assert_eq!(span_count(&statement_spans, "sql", "insert_row"), 10_000);
    assert_eq!(span_count(&batched_spans, "sql", "insert_rows"), 1);
    assert!(
        batched_ms < 50.0,
        "batched insert {batched_ms:.1} ms, want under 50 ms"
    );
    assert!(
        per_statement_ms >= 20.0 * batched_ms,
        "per-statement {per_statement_ms:.1} ms, want >= 20x the batched {batched_ms:.1} ms"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn autocommit_costs_ten_times_one_explicit_transaction() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("autocommit");
    let inserts = |connection: &Connection| {
        for n in 0..rows {
            insert_values(connection, "t", &[n], "insert_row");
        }
    };

    let autocommit_spans = traced(|| {
        let connection = open(&directory.join("autocommit.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        inserts(&connection);
    });

    let transaction_spans = traced(|| {
        let connection = open(&directory.join("transaction.sqlite")).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        inserts(&connection);
        commit(&connection);
    });

    let autocommit_ms = span_ms(&autocommit_spans, "sql", "insert_row");
    let transaction_ms = span_ms(&transaction_spans, "sql", "insert_row");
    println!("autocommit {autocommit_ms:.1} ms, one transaction {transaction_ms:.1} ms");

    assert_eq!(
        span_count(&autocommit_spans, "sql", "insert_row"),
        rows as usize,
        "autocommit span count must equal the statement count"
    );
    assert_eq!(
        span_count(&transaction_spans, "sql", "insert_row"),
        rows as usize,
        "transaction span count must equal the statement count"
    );
    assert!(
        autocommit_ms >= 10.0 * transaction_ms,
        "autocommit {autocommit_ms:.1} ms, want >= 10x the one-transaction {transaction_ms:.1} ms"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// The `EXPLAIN QUERY PLAN` detail lines of one probe, indexed then not.
fn probe_plans(connection: &Connection, value: i64) -> (Vec<String>, Vec<String>) {
    let explain = |select: &str| {
        sql(connection, "explain", |connection| {
            let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {select}"))?;
            let mut cursor = statement.query([value])?;
            let mut detail = Vec::new();
            while let Some(row) = cursor.next()? {
                detail.push(row.get::<_, String>(3)?);
            }
            let count = detail.len();
            Ok((detail, count))
        })
        .unwrap()
    };
    let indexed = explain("SELECT \"n\" FROM \"lookup\" WHERE \"n\" = ?");
    let unindexed = explain("SELECT \"n\" FROM \"lookup\" WHERE \"payload\" = ?");
    (indexed, unindexed)
}

#[test]
fn unindexed_lookup_degrades_five_times_faster_at_hundred_k() {
    let _lane = lane();
    let directory = directory("lookup");
    let probes = 1_000i64;

    let probe_ms = |rows: i64| -> (f64, f64) {
        let spans = traced(|| {
            let connection = open(&directory.join("lookup.sqlite")).unwrap();
            declare(
                &connection,
                "CREATE TABLE IF NOT EXISTS \"lookup\" \
                 (\"n\" INTEGER NOT NULL, \"payload\" INTEGER NOT NULL)",
            );
            let mut filled = sql(&connection, "count_rows", |connection| {
                let rows: i64 =
                    connection
                        .query_row("SELECT count(*) FROM \"lookup\"", (), |row| row.get(0))?;
                Ok((rows, 1))
            })
            .unwrap();
            if filled == 0 {
                declare(
                    &connection,
                    "CREATE INDEX \"lookup_n\" ON \"lookup\" (\"n\")",
                );
            }
            while filled < rows {
                let chunk: Vec<i64> = (filled..rows.min(filled + 15_000)).collect();
                begin(&connection);
                let pairs: Vec<(i64, i64)> = chunk.iter().map(|n| (*n, *n)).collect();
                insert_pairs(&connection, "lookup", "\"n\", \"payload\"", &pairs);
                commit(&connection);
                filled += chunk.len() as i64;
            }
            for probe in 0..probes {
                let value = probe * rows / probes;
                sql(&connection, "probe_indexed", |connection| {
                    let found = connection.query_row(
                        "SELECT \"n\" FROM \"lookup\" WHERE \"n\" = ?",
                        [value],
                        |row| row.get::<_, i64>(0),
                    )?;
                    Ok((found, 1))
                })
                .unwrap();
                sql(&connection, "probe_unindexed", |connection| {
                    let found = connection.query_row(
                        "SELECT \"n\" FROM \"lookup\" WHERE \"payload\" = ?",
                        [value],
                        |row| row.get::<_, i64>(0),
                    )?;
                    Ok((found, 1))
                })
                .unwrap();
            }
        });
        (
            span_ms(&spans, "sql", "probe_unindexed"),
            span_ms(&spans, "sql", "probe_indexed"),
        )
    };

    let (unindexed_10k, indexed_10k) = probe_ms(10_000);
    let (unindexed_100k, indexed_100k) = probe_ms(100_000);
    let ratio_10k = unindexed_10k / indexed_10k;
    let ratio_100k = unindexed_100k / indexed_100k;
    println!(
        "ratio at 10k {ratio_10k:.1}x ({unindexed_10k:.1} ms vs {indexed_10k:.1} ms), at 100k {ratio_100k:.1}x ({unindexed_100k:.1} ms vs {indexed_100k:.1} ms)"
    );

    // The plans are asserted, never timed: SCAN against the bare column,
    // SEARCH against the index.
    let connection = open(&directory.join("lookup.sqlite")).unwrap();
    let (indexed, unindexed) = probe_plans(&connection, 5_000);
    let joined = |plans: &[String]| plans.join(" | ");
    assert!(
        indexed.iter().any(|plan| plan.contains("SEARCH")),
        "indexed plan must SEARCH: {}",
        joined(&indexed)
    );
    assert!(
        unindexed.iter().any(|plan| plan.contains("SCAN")),
        "unindexed plan must SCAN: {}",
        joined(&unindexed)
    );
    assert!(
        ratio_100k >= 5.0 * ratio_10k,
        "ratio at 100k {ratio_100k:.1}x, want >= 5x the ratio at 10k {ratio_10k:.1}x"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn join_uses_the_index_on_the_inner_side() {
    let _lane = lane();
    let rows = 10_000i64;
    let directory = directory("join");
    let mut without = Vec::new();
    let mut with = Vec::new();
    traced(|| {
        let connection = open(&directory.join("join.sqlite")).unwrap();
        declare(
            &connection,
            "CREATE TABLE \"j_a\" (\"n\" INTEGER NOT NULL, \"b_id\" INTEGER NOT NULL);
             CREATE TABLE \"j_b\" (\"n\" INTEGER NOT NULL, \"tag\" INTEGER NOT NULL)",
        );
        begin(&connection);
        let a_pairs: Vec<(i64, i64)> = (0..rows).map(|n| (n, n)).collect();
        insert_pairs(&connection, "j_a", "\"n\", \"b_id\"", &a_pairs);
        insert_pairs(&connection, "j_b", "\"n\", \"tag\"", &a_pairs);
        commit(&connection);
        let join = "SELECT count(*) FROM \"j_a\" JOIN \"j_b\" \
                    ON \"j_a\".\"b_id\" = \"j_b\".\"n\" WHERE \"j_b\".\"tag\" < 100";
        let explain = |label: &'static str| {
            sql(&connection, label, |connection| {
                let mut prepared = connection.prepare(&format!("EXPLAIN QUERY PLAN {join}"))?;
                let mut cursor = prepared.query(())?;
                let mut detail = Vec::new();
                while let Some(row) = cursor.next()? {
                    detail.push(row.get::<_, String>(3)?);
                }
                let count = detail.len();
                Ok((detail, count))
            })
            .unwrap()
        };
        without = explain("explain_join_unindexed");
        declare(&connection, "CREATE INDEX \"j_b_n\" ON \"j_b\" (\"n\")");
        with = explain("explain_join_indexed");
    });
    let joined = |plans: &[String]| plans.join(" | ");
    assert!(
        without.iter().any(|plan| plan.contains("SCAN")),
        "unindexed join must SCAN somewhere: {}",
        joined(&without)
    );
    assert!(
        with.iter()
            .any(|plan| plan.contains("SEARCH j_b USING INDEX")),
        "indexed join must SEARCH j_b: {}",
        joined(&with)
    );
    assert!(
        !with.iter().any(|plan| plan.contains("SCAN j_b")),
        "indexed join must not SCAN the inner side: {}",
        joined(&with)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn contract_pragmas_read_back_from_a_file_db() {
    let _lane = lane();
    let directory = directory("pragmas");
    let connection = open(&directory.join("contract.sqlite")).unwrap();
    let read = |pragma: &str| -> String {
        let value: rusqlite::types::Value = connection
            .query_row(&format!("PRAGMA {pragma}"), (), |row| row.get(0))
            .unwrap();
        match value {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(text) => text,
            other => panic!("unexpected value for {pragma}: {other:?}"),
        }
    };
    assert_eq!(read("page_size"), "65536");
    assert_eq!(read("journal_mode"), "wal");
    assert_eq!(read("synchronous"), "1");
    assert_eq!(read("mmap_size"), "1073741824");
    assert_eq!(read("cache_size"), "-262144");
    assert_eq!(read("temp_store"), "2");
    assert_eq!(read("recursive_triggers"), "1");
    assert_eq!(read("trusted_schema"), "1");

    let memory = open(Path::new(":memory:")).unwrap();
    let mode: String = memory
        .query_row("PRAGMA journal_mode", (), |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "memory", "an in-memory db keeps its own journal mode");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn cache_size_is_the_memory_ceiling() {
    let _lane = lane();
    let megabyte = 1024.0 * 1024.0;
    let payload = "x".repeat(16 * 1024);
    let rows = 4_096usize;
    let directory = directory("cache");

    let run = |file: &str, cache_kib: i64, before_rss: i64| -> (f64, i64) {
        let spans = traced(|| {
            let connection = open(&directory.join(file)).unwrap();
            sql(&connection, "cache_size", |connection| {
                connection
                    .execute_batch(&format!(
                        "PRAGMA mmap_size = 0; PRAGMA cache_size = {cache_kib};"
                    ))
                    .map(|()| ((), 0))
            })
            .unwrap();
            declare(
                &connection,
                "CREATE TABLE \"blob\" (\"n\" INTEGER NOT NULL, \"payload\" TEXT NOT NULL)",
            );
            begin(&connection);
            for chunk in 0..(rows / 32) {
                sql(&connection, "insert_blob_rows", |connection| {
                    let tuples = vec!["(?, ?)"; 32].join(",");
                    let mut values: Vec<rusqlite::types::Value> = Vec::with_capacity(64);
                    for n in (chunk * 32) as i64..((chunk + 1) * 32) as i64 {
                        values.push(rusqlite::types::Value::Integer(n));
                        values.push(rusqlite::types::Value::Text(payload.clone()));
                    }
                    let written = connection.execute(
                        &format!("INSERT INTO \"blob\" (\"n\", \"payload\") VALUES {tuples}"),
                        rusqlite::params_from_iter(values.iter()),
                    )?;
                    Ok((written, written))
                })
                .unwrap();
            }
            commit(&connection);
        });
        let ms = span_ms(&spans, "sql", "insert_blob_rows");
        let peak = max_rss_bytes();
        (ms, peak - before_rss)
    };

    let before = max_rss_bytes();
    let (ms_8mib, rss_8mib) = run("cache8.sqlite", -8192, before);
    let after_8 = max_rss_bytes();
    let (ms_256mib, rss_256mib) = run("cache256.sqlite", -262144, after_8);

    println!(
        "8 MiB cache: {ms_8mib:.1} ms, rss {:+.1} MiB; 256 MiB cache: {ms_256mib:.1} ms, rss {:+.1} MiB",
        rss_8mib as f64 / megabyte,
        rss_256mib as f64 / megabyte
    );

    assert!(
        rss_8mib as f64 <= 8.0 * megabyte + 32.0 * megabyte,
        "8 MiB cache run grew RSS {:.1} MiB, want under 40 MiB",
        rss_8mib as f64 / megabyte
    );
    assert!(
        rss_256mib as f64 <= 256.0 * megabyte + 32.0 * megabyte,
        "256 MiB cache run grew RSS {:.1} MiB, want under 288 MiB",
        rss_256mib as f64 / megabyte
    );
    assert!(
        ms_256mib <= ms_8mib,
        "256 MiB cache {ms_256mib:.1} ms, want no slower than the 8 MiB cache {ms_8mib:.1} ms"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn wal_checkpoint_bounds_the_wal() {
    let _lane = lane();
    let directory = directory("wal");
    let file = directory.join("wal.sqlite");
    let mut wal_before = 0u64;
    let spans = traced(|| {
        let connection = open(&file).unwrap();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        begin(&connection);
        let mut filled = 0i64;
        while filled < 50_000 {
            let chunk: Vec<i64> = (filled..50_000.min(filled + 25_000)).collect();
            insert_values(&connection, "t", &chunk, "insert_rows");
            filled += chunk.len() as i64;
        }
        commit(&connection);
        wal_before = std::fs::metadata(directory.join("wal.sqlite-wal"))
            .map(|meta| meta.len())
            .unwrap_or(0);
        sql(&connection, "wal_checkpoint", |connection| {
            let mut statement = connection.prepare("PRAGMA wal_checkpoint(TRUNCATE)")?;
            let mut cursor = statement.query(())?;
            let mut row = Vec::new();
            while let Some(line) = cursor.next()? {
                row.push(line.get::<_, i64>(0)?);
            }
            Ok((row, 1))
        })
        .unwrap();
    });
    assert!(
        wal_before > 0,
        "the wal must hold pages before the checkpoint truncates it"
    );
    assert_eq!(
        span_count(&spans, "sql", "wal_checkpoint"),
        1,
        "the checkpoint runs inside one sql span"
    );
    let wal_after = std::fs::metadata(directory.join("wal.sqlite-wal"))
        .map(|meta| meta.len())
        .unwrap_or(0);
    assert_eq!(wal_after, 0, "wal_checkpoint(TRUNCATE) must empty the wal");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn rust_time_is_separable_from_sql_time() {
    let _lane = lane();
    let directory = directory("rust_ms");
    let connection = open(&directory.join("phases.sqlite")).unwrap();
    let spans = traced(|| {
        let phase = tracing::info_span!("phase", name = "contract");
        let _entered = phase.enter();
        declare(&connection, "CREATE TABLE \"t\" (\"n\" INTEGER NOT NULL)");
        let values: Vec<i64> = (0..1_000).collect();
        insert_values(&connection, "t", &values, "insert_rows");
        sql(&connection, "count_rows", |connection| {
            let rows: i64 =
                connection.query_row("SELECT count(*) FROM \"t\"", (), |row| row.get(0))?;
            Ok((rows, 1))
        })
        .unwrap();
    });

    let phase: Vec<&RecordedSpan> = spans.iter().filter(|span| span.kind == "phase").collect();
    assert_eq!(phase.len(), 1, "one phase span");
    let phase_ms = phase[0].elapsed_ms.unwrap();
    let sql_total: f64 = spans
        .iter()
        .filter(|span| span.kind == "sql")
        .map(|span| span.ms.unwrap_or(0.0))
        .sum();
    let rust_ms = phase_ms - sql_total;
    println!("phase {phase_ms:.1} ms, sql {sql_total:.1} ms, rust {rust_ms:.1} ms");
    assert!(sql_total > 0.0, "the sql spans must carry their ms");
    assert!(
        rust_ms >= 0.0,
        "rust ms {rust_ms:.1} below zero: sql spans {sql_total:.1} exceed the phase {phase_ms:.1}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}
