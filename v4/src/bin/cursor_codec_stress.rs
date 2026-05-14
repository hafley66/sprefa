use std::env;
use std::sync::Arc;
use std::time::Instant;

use effect_runtime::v2::Next;
use serde::{Deserialize, Serialize};
use v4::cursor_codec;
use v4::{Cursor, CursorValue, Ref, StringId, Term, WhereBytesId};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonCursor {
    terms: Vec<JsonTerm>,
}

#[derive(Clone, Debug, Serialize)]
struct JsonCursorRef<'a> {
    terms: Vec<JsonTermRef<'a>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonTerm {
    name: String,
    value: String,
    cursor_value_tag: u8,
    cursor_value_payload: u64,
    value_id: u64,
    at: u64,
}

#[derive(Clone, Debug, Serialize)]
struct JsonTermRef<'a> {
    name: &'a str,
    value: &'a str,
    cursor_value_tag: u8,
    cursor_value_payload: u64,
    value_id: u64,
    at: u64,
}

#[derive(Clone, Copy, Debug)]
struct Args {
    rows: usize,
}

fn main() {
    let args = parse_args();
    let started = Instant::now();

    let (cursors, gen_ms) = timed(|| {
        (0..args.rows)
            .map(make_join_like_cursor)
            .collect::<Vec<_>>()
    });
    println!(
        "rows={} gen_ms={:.1} rss_peak_MB={}",
        args.rows,
        gen_ms,
        rss_peak_mb()
    );

    let (binary_blobs, binary_encode_ms) =
        timed(|| cursors.iter().map(cursor_codec::encode).collect::<Vec<_>>());
    print_blob_stats("binary_encode", args.rows, binary_encode_ms, &binary_blobs);

    let (binary_decode_checksum, binary_decode_ms) = timed(|| {
        binary_blobs
            .iter()
            .map(|blob| checksum_cursor(&cursor_codec::decode(blob).expect("binary decode")))
            .fold(0_u64, |a, b| a ^ b)
    });
    print_op_stats(
        "binary_decode",
        args.rows,
        binary_decode_ms,
        blob_bytes(&binary_blobs),
        binary_decode_checksum,
    );

    let (binary_encode_hash_checksum, binary_encode_hash_ms) = timed(|| {
        cursors
            .iter()
            .map(|cursor| {
                let blob = cursor_codec::encode(cursor);
                let h = blake3::hash(&blob);
                blob.len() as u64 ^ u64::from_le_bytes(h.as_bytes()[0..8].try_into().unwrap())
            })
            .fold(0_u64, |a, b| a ^ b)
    });
    print_op_stats(
        "binary_encode_hash_single_pass",
        args.rows,
        binary_encode_hash_ms,
        blob_bytes(&binary_blobs),
        binary_encode_hash_checksum,
    );

    let (binary_queue_checksum, binary_queue_ms) = timed(|| {
        cursors
            .iter()
            .map(|cursor| {
                let blob = cursor_codec::encode(cursor);
                let h = cursor.content_hash();
                blob.len() as u64 ^ u64::from_le_bytes(h[0..8].try_into().unwrap())
            })
            .fold(0_u64, |a, b| a ^ b)
    });
    print_op_stats(
        "binary_queue_current_encode_plus_content_hash",
        args.rows,
        binary_queue_ms,
        blob_bytes(&binary_blobs),
        binary_queue_checksum,
    );

    let (json_blobs, json_encode_ms) =
        timed(|| cursors.iter().map(json_encode).collect::<Vec<_>>());
    print_blob_stats("json_encode", args.rows, json_encode_ms, &json_blobs);

    let (json_decode_checksum, json_decode_ms) = timed(|| {
        json_blobs
            .iter()
            .map(|blob| checksum_cursor(&json_decode(blob)))
            .fold(0_u64, |a, b| a ^ b)
    });
    print_op_stats(
        "json_decode",
        args.rows,
        json_decode_ms,
        blob_bytes(&json_blobs),
        json_decode_checksum,
    );

    let (json_encode_hash_checksum, json_encode_hash_ms) = timed(|| {
        cursors
            .iter()
            .map(|cursor| {
                let blob = json_encode(cursor);
                let h = blake3::hash(&blob);
                blob.len() as u64 ^ u64::from_le_bytes(h.as_bytes()[0..8].try_into().unwrap())
            })
            .fold(0_u64, |a, b| a ^ b)
    });
    print_op_stats(
        "json_encode_hash_single_pass",
        args.rows,
        json_encode_hash_ms,
        blob_bytes(&json_blobs),
        json_encode_hash_checksum,
    );

    println!(
        "total_ms={:.1} rss_peak_MB={}",
        started.elapsed().as_secs_f64() * 1000.0,
        rss_peak_mb()
    );
}

fn parse_args() -> Args {
    let mut rows = 706_778;
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--rows" => {
                rows = it
                    .next()
                    .expect("--rows value")
                    .parse()
                    .expect("--rows usize");
            }
            _ => panic!("unknown arg {arg}; use --rows N"),
        }
    }
    Args { rows }
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

fn encode_cursor_value(value: CursorValue) -> (u8, u64) {
    match value {
        CursorValue::Null => (0, 0),
        CursorValue::Bool(v) => (1, u64::from(v)),
        CursorValue::Int(v) => (2, u64::from_le_bytes(v.to_le_bytes())),
        CursorValue::Float(v) => (3, v),
        CursorValue::String(v) => (4, v.0),
        CursorValue::WhereBytes(v) => (5, v.0),
        CursorValue::Blob(v) => (6, v),
    }
}

fn decode_cursor_value(tag: u8, payload: u64) -> CursorValue {
    match tag {
        0 => CursorValue::Null,
        1 => CursorValue::Bool(payload != 0),
        2 => CursorValue::Int(i64::from_le_bytes(payload.to_le_bytes())),
        3 => CursorValue::Float(payload),
        4 => CursorValue::String(StringId(payload)),
        5 => CursorValue::WhereBytes(WhereBytesId(payload)),
        6 => CursorValue::Blob(payload),
        _ => panic!("unknown cursor_value tag {tag}"),
    }
}

fn json_encode(cursor: &Cursor) -> Vec<u8> {
    let json = JsonCursorRef {
        terms: cursor
            .terms
            .iter()
            .map(|term| {
                let (cursor_value_tag, cursor_value_payload) =
                    encode_cursor_value(term.cursor_value);
                JsonTermRef {
                    name: term.name.as_ref(),
                    value: term.value.as_ref(),
                    cursor_value_tag,
                    cursor_value_payload,
                    value_id: term.value_id.0,
                    at: term.at.0,
                }
            })
            .collect(),
    };
    serde_json::to_vec(&json).expect("json encode")
}

fn json_decode(bytes: &[u8]) -> Cursor {
    let json: JsonCursor = serde_json::from_slice(bytes).expect("json decode");
    let mut cursor = Cursor::default();
    cursor.terms = json
        .terms
        .into_iter()
        .map(|term| Term {
            name: Arc::<str>::from(term.name),
            value: Arc::<str>::from(term.value),
            cursor_value: decode_cursor_value(term.cursor_value_tag, term.cursor_value_payload),
            value_id: StringId(term.value_id),
            at: Ref(term.at),
        })
        .collect();
    cursor
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

fn timed<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let value = f();
    (value, started.elapsed().as_secs_f64() * 1000.0)
}

fn blob_bytes(blobs: &[Vec<u8>]) -> usize {
    blobs.iter().map(Vec::len).sum()
}

fn print_blob_stats(label: &str, rows: usize, ms: f64, blobs: &[Vec<u8>]) {
    let bytes = blob_bytes(blobs);
    println!(
        "{label} ms={:.1} rows_per_sec={:.0} MB={:.1} avg_bytes={:.1} rss_peak_MB={}",
        ms,
        rows as f64 / (ms / 1000.0),
        bytes as f64 / 1_000_000.0,
        bytes as f64 / rows as f64,
        rss_peak_mb()
    );
}

fn print_op_stats(label: &str, rows: usize, ms: f64, bytes: usize, checksum: u64) {
    println!(
        "{label} ms={:.1} rows_per_sec={:.0} MB={:.1} MB_per_sec={:.1} checksum={checksum} rss_peak_MB={}",
        ms,
        rows as f64 / (ms / 1000.0),
        bytes as f64 / 1_000_000.0,
        (bytes as f64 / 1_000_000.0) / (ms / 1000.0),
        rss_peak_mb()
    );
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
