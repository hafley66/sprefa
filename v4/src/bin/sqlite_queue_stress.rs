use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use effect_runtime::v2::{QueueBackend, QueueRow, SqliteQueue, Wake};
use rusqlite::{params, Connection};
use tracing_subscriber::EnvFilter;
use v4::cursor_codec;
use v4::Cursor;

#[derive(Clone, Debug)]
struct Args {
    rows: usize,
    batches: Vec<usize>,
    db: PathBuf,
}

#[derive(Default)]
struct DrainStats {
    batches: usize,
    rows: usize,
    select_time: Duration,
    decode_time: Duration,
    delete_time: Duration,
    total_time: Duration,
    blob_bytes: usize,
    checksum: u64,
}

fn main() {
    let args = parse_args();
    init_tracing();

    println!(
        "settings rows={} batches={} db={} sqlite_bulk_enqueue_multirow={} queue_kind=sqlite file journal_mode=WAL wake_kind=immediate pipe_hash=0 instance_id=0 depth=0 cursor_shape=join_like path_u32s=2",
        args.rows,
        args.batches
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(","),
        args.db.display(),
        sqlite_bulk_enqueue_multirow(),
    );

    for batch in args.batches.iter().copied() {
        run_one_batch(&args, batch);
    }
}

fn run_one_batch(args: &Args, batch: usize) {
    let _ = fs::remove_file(&args.db);
    let queue = SqliteQueue::<Cursor>::open_file(&args.db);

    let (rows, gen_ms) = timed(|| make_rows(args.rows));
    println!(
        "rows={} batch={} db={} gen_ms={:.1} rss_peak_MB={}",
        args.rows,
        batch,
        args.db.display(),
        gen_ms,
        rss_peak_mb()
    );

    let (_, enqueue_ms) = timed(|| queue.bulk_enqueue(rows));
    print_db_stats("after_enqueue", &args.db);
    println!(
        "queue_api bulk_enqueue rows={} ms={:.1} rows_per_sec={:.0} rss_peak_MB={}",
        args.rows,
        enqueue_ms,
        rows_per_sec(args.rows, enqueue_ms),
        rss_peak_mb()
    );

    let raw = raw_drain(queue.connection(), batch);
    print_drain_stats("raw_sqlite_drain", batch, &raw);
    print_db_stats("after_raw_drain", &args.db);

    let (_, refill_ms) = timed(|| queue.bulk_enqueue(make_rows(args.rows)));
    print_db_stats("after_refill", &args.db);
    println!(
        "queue_api refill_bulk_enqueue rows={} ms={:.1} rows_per_sec={:.0} rss_peak_MB={}",
        args.rows,
        refill_ms,
        rows_per_sec(args.rows, refill_ms),
        rss_peak_mb()
    );

    let api = queue_api_drain(&queue, batch);
    print_drain_stats("queue_api_drain", batch, &api);
    print_db_stats("after_queue_api_drain", &args.db);
}

fn parse_args() -> Args {
    let mut rows = 706_778;
    let mut batches = vec![65_536];
    let mut db = PathBuf::from("/private/tmp/sprefa-sqlite-queue-stress.db");
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--rows" => rows = it.next().expect("--rows value").parse().expect("usize"),
            "--batch" => batches = vec![it.next().expect("--batch value").parse().expect("usize")],
            "--batches" => {
                batches = it
                    .next()
                    .expect("--batches value")
                    .split(',')
                    .map(|s| s.parse().expect("usize batch"))
                    .collect();
            }
            "--db" => db = PathBuf::from(it.next().expect("--db value")),
            _ => panic!("unknown arg {arg}; use --rows N --batch N|--batches A,B --db PATH"),
        }
    }
    Args { rows, batches, db }
}

fn init_tracing() {
    let filter =
        env::var("RUST_LOG").unwrap_or_else(|_| "effect_runtime::sqlite_queue=info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(true)
        .without_time()
        .try_init();
}

fn make_rows(n: usize) -> Vec<QueueRow<Cursor>> {
    (0..n)
        .map(|i| QueueRow {
            id: 0,
            parent_id: None,
            batch_idx: (i % 65_536) as u32,
            path: vec![0, 1],
            pipe_hash: 0,
            instance_id: 0,
            depth: 0,
            value: Arc::new(make_join_like_cursor(i)),
            wake: Wake::Immediate,
            expand_tick: 1,
            enqueued_at_ns: 0,
        })
        .collect()
}

fn make_join_like_cursor(i: usize) -> Cursor {
    let file = format!("/linux/kernel/{:03}/driver_{:05}.c", i % 4096, i % 63_482);
    let lo = ((i * 37) % 1_700_000).to_string();
    let hi = ((i * 37) % 1_700_000 + 19).to_string();
    let other_lo = ((i * 13 + 7) % 1_700_000).to_string();
    let line = format!("printk candidate {:06}", i % 1_000_000);

    let mut c = Cursor::default();
    c.set_value(line);
    c.set("FS", file);
    c.set("LO", lo);
    c.set("HI", hi);
    c.set("OTHER_LO", other_lo);
    c
}

fn raw_drain(conn: Arc<std::sync::Mutex<Connection>>, batch: usize) -> DrainStats {
    let total_t0 = Instant::now();
    let mut stats = DrainStats::default();

    loop {
        let select_t0 = Instant::now();
        let rows = {
            let conn = conn.lock().unwrap();
            let mut stmt = conn
                .prepare_cached(
                    "SELECT id, next_blob
                     FROM sprf_v3_queue
                     WHERE pipe_hash = ?1
                       AND instance_id = ?2
                       AND depth = ?3
                       AND wake_kind = ?4
                     ORDER BY id ASC
                     LIMIT ?5",
                )
                .expect("prepare raw select");
            let rows = stmt
                .query_map(params![0_i64, 0_i64, 0_i64, 0_i64, batch as i64], |row| {
                    Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
                })
                .expect("raw select")
                .map(|row| row.expect("raw row"))
                .collect::<Vec<_>>();
            rows
        };
        stats.select_time += select_t0.elapsed();
        if rows.is_empty() {
            break;
        }

        stats.batches += 1;
        stats.rows += rows.len();
        stats.blob_bytes += rows.iter().map(|(_, blob)| blob.len()).sum::<usize>();

        let decode_t0 = Instant::now();
        for (_, blob) in &rows {
            stats.checksum ^= checksum_cursor(&cursor_codec::decode(blob).expect("cursor decode"));
        }
        stats.decode_time += decode_t0.elapsed();

        let delete_t0 = Instant::now();
        delete_ids(&conn, rows.iter().map(|(id, _)| *id));
        stats.delete_time += delete_t0.elapsed();
    }

    stats.total_time = total_t0.elapsed();
    stats
}

fn queue_api_drain(queue: &SqliteQueue<Cursor>, batch: usize) -> DrainStats {
    let total_t0 = Instant::now();
    let mut stats = DrainStats::default();

    loop {
        let pull_t0 = Instant::now();
        let rows = queue.pull_runnable_batch_for(0, 0, 1, batch);
        let pull_time = pull_t0.elapsed();
        if rows.is_empty() {
            break;
        }
        stats.batches += 1;
        stats.rows += rows.len();
        stats.select_time += pull_time;
        for row in rows {
            stats.checksum ^= checksum_cursor(row.value.as_ref());
        }
    }

    stats.total_time = total_t0.elapsed();
    stats
}

fn delete_ids(conn: &Arc<std::sync::Mutex<Connection>>, ids: impl IntoIterator<Item = u64>) {
    let ids = ids.into_iter().collect::<Vec<_>>();
    if ids.is_empty() {
        return;
    }
    let sql = format!(
        "DELETE FROM sprf_v3_queue WHERE id IN ({})",
        ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
    );
    conn.lock().unwrap().execute(&sql, []).expect("raw delete");
}

fn checksum_cursor(cursor: &Cursor) -> u64 {
    cursor.terms.iter().fold(0_u64, |acc, term| {
        acc.wrapping_add(term.name.len() as u64)
            .wrapping_mul(31)
            .wrapping_add(term.value.len() as u64)
            .wrapping_add(term.value_id.0)
            .wrapping_add(term.at.0)
    })
}

fn print_drain_stats(name: &str, batch: usize, stats: &DrainStats) {
    println!(
        "{name} batch_limit={} batches={} avg_batch={:.1} rows={} rows_per_sec={:.0} select_or_pull_ms={:.1} select_or_pull_rows_per_sec={:.0} decode_ms={:.1} decode_rows_per_sec={:.0} decode_MB_per_sec={:.1} delete_ms={:.1} delete_rows_per_sec={:.0} total_ms={:.1} blob_MB={:.1} checksum={} rss_peak_MB={}",
        batch,
        stats.batches,
        if stats.batches == 0 {
            0.0
        } else {
            stats.rows as f64 / stats.batches as f64
        },
        stats.rows,
        per_sec(stats.rows, stats.total_time),
        stats.select_time.as_secs_f64() * 1000.0,
        per_sec(stats.rows, stats.select_time),
        stats.decode_time.as_secs_f64() * 1000.0,
        per_sec(stats.rows, stats.decode_time),
        mb_per_sec(stats.blob_bytes, stats.decode_time),
        stats.delete_time.as_secs_f64() * 1000.0,
        per_sec(stats.rows, stats.delete_time),
        stats.total_time.as_secs_f64() * 1000.0,
        stats.blob_bytes as f64 / (1024.0 * 1024.0),
        stats.checksum,
        rss_peak_mb()
    );
}

fn sqlite_bulk_enqueue_multirow() -> usize {
    2_048
}

fn rows_per_sec(rows: usize, ms: f64) -> f64 {
    if ms == 0.0 {
        0.0
    } else {
        rows as f64 / (ms / 1000.0)
    }
}

fn per_sec(count: usize, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        0.0
    } else {
        count as f64 / secs
    }
}

fn mb_per_sec(bytes: usize, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        0.0
    } else {
        bytes as f64 / (1024.0 * 1024.0) / secs
    }
}

fn print_db_stats(label: &str, path: &PathBuf) {
    let Ok(conn) = Connection::open(path) else {
        return;
    };
    let file_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let page_count = conn
        .query_row("PRAGMA page_count", [], |row| row.get::<_, u64>(0))
        .unwrap_or(0);
    let page_size = conn
        .query_row("PRAGMA page_size", [], |row| row.get::<_, u64>(0))
        .unwrap_or(4096);
    let rows = conn
        .query_row("SELECT COUNT(*) FROM sprf_v3_queue", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap_or(0);
    println!(
        "{label} db_file_MB={:.1} db_page_MB={:.1} queue_rows={rows}",
        file_bytes as f64 / (1024.0 * 1024.0),
        (page_count * page_size) as f64 / (1024.0 * 1024.0),
    );
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let value = f();
    (value, started.elapsed().as_secs_f64() * 1000.0)
}

fn rss_peak_mb() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return 0;
    }
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    {
        (usage.ru_maxrss as u64) / 1_000_000
    }
    #[cfg(not(target_os = "macos"))]
    {
        (usage.ru_maxrss as u64) / 1_024
    }
}
