//! The tracing seam: stderr silence by default, and the summary layer's table.
#![cfg(feature = "cli")]

use std::process::Command;
use std::sync::Arc;

use sprefa_extract::trace::{SummaryLayer, SummaryState};
use sprefa_extract::{dispatch, FamilyMask};
use tracing_subscriber::{layer::SubscriberExt, Registry};

const BIN: &str = env!("CARGO_BIN_EXE_extract");
const FIXTURE: &str = "tests/fixtures/rust/sample.rs";

/// The family table's body: from its wall line to the blank line before the
/// phase table.
fn family_body(table: &str) -> Vec<&str> {
    table
        .lines()
        .skip_while(|line| !line.starts_with("extract summary: wall "))
        .skip(2)
        .take_while(|line| !line.trim().is_empty())
        .collect()
}

/// The phase table's body, from its load line.
fn phase_body(table: &str) -> Vec<&str> {
    table
        .lines()
        .skip_while(|line| !line.starts_with("extract phases: load "))
        .skip(2)
        .take_while(|line| !line.trim().is_empty())
        .collect()
}

/// The (lang, family) pairs the rendered table names, header row dropped.
fn table_rows(table: &str) -> Vec<(String, String)> {
    family_body(table)
        .into_iter()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_string(), fields.next()?.to_string()))
        })
        .collect()
}

/// One (lang, phase) row of the phase table as (files, calls, rows), or None
/// when that phase was never entered.
fn phase_row(table: &str, lang: &str, phase: &str) -> Option<(u64, u64, u64)> {
    phase_body(table).into_iter().find_map(|line| {
        let columns: Vec<&str> = line.split_whitespace().collect();
        (columns.len() == 7 && columns[0] == lang && columns[1] == phase).then(|| {
            (
                columns[2].parse().unwrap(),
                columns[3].parse().unwrap(),
                columns[4].parse().unwrap(),
            )
        })
    })
}

/// One `--family cst,type,call` run over one file, phase table on stderr and
/// the trail off, so a fixture run never touches `~/.agent`.
fn phases_of(path: &str) -> String {
    let output = Command::new(BIN)
        .args(["--family", "cst,type,call", path])
        .env_remove("RUST_LOG")
        .env("DL_TRACE_SUMMARY", "1")
        .env("DL_TRAIL", "0")
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed on {path}");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

// FAIL-FIRST RECEIPT: a subscriber installed with a default-on filter put a
// CLOSE line per span on stderr for every file, under no flag at all.
#[test]
fn no_rust_log_means_no_stderr_byte() {
    let output = Command::new(BIN)
        .args(["--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env_remove("DL_TRACE_SUMMARY")
        .env_remove("HAFLEY_LOG_FORMAT")
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    assert!(
        !output.stdout.is_empty(),
        "the fact stream must still reach stdout"
    );
    assert_eq!(
        output.stderr,
        Vec::<u8>::new(),
        "stderr must be empty with RUST_LOG unset, got {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn json_format_emits_service_version_and_process_identity() {
    let output = Command::new(BIN)
        .args(["--family", "call", FIXTURE])
        .env("RUST_LOG", "sprefa_extract=debug,hafley_observe=debug")
        .env("HAFLEY_LOG_FORMAT", "json")
        .env_remove("DL_TRACE_SUMMARY")
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    let values = String::from_utf8(output.stderr)
        .expect("JSON log is UTF-8")
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).expect("one JSON event per line")
        })
        .collect::<Vec<_>>();
    let startup = values
        .iter()
        .find(|value| value["fields"]["message"] == "observability initialized")
        .expect("startup event");
    assert_eq!(
        serde_json::json!({
            "service.name": startup["fields"]["service.name"],
            "service.version": startup["fields"]["service.version"],
            "process.pid": startup["fields"]["process.pid"],
            "log.format": startup["fields"]["log.format"],
        }),
        serde_json::json!({
            "service.name": "sprefa-extract",
            "service.version": env!("CARGO_PKG_VERSION"),
            "process.pid": startup["fields"]["process.pid"],
            "log.format": "json",
        })
    );
    assert!(startup["fields"]["process.pid"].as_u64().is_some());
}

#[test]
fn summary_layer_renders_a_row_per_lang_and_family() {
    let state = Arc::new(SummaryState::new());
    let subscriber = Registry::default().with(SummaryLayer::new(Arc::clone(&state)));
    let content = std::fs::read(FIXTURE).expect("read fixture");
    tracing::subscriber::with_default(subscriber, || {
        dispatch(FIXTURE, &content, FamilyMask::ALL).expect("rust source matches");
    });
    let table = state.render();
    let rows = table_rows(&table);
    for family in ["parse", "cst", "type", "call", "df"] {
        assert!(
            rows.contains(&("rust".to_string(), family.to_string())),
            "no rust/{family} row in\n{table}"
        );
    }
    assert!(
        rows.iter().any(|(_, family)| family == "extract_file"),
        "no extract_file row in\n{table}"
    );
    assert!(
        table.starts_with("extract summary: wall "),
        "table must open with the wall line, got\n{table}"
    );
}

#[test]
fn summary_flag_prints_the_table_to_stderr() {
    let output = Command::new(BIN)
        .args(["--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env("DL_TRACE_SUMMARY", "1")
        .env("DL_TRAIL", "0")
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("extract summary: wall "),
        "no summary table on stderr, got {stderr}"
    );
    let rows = table_rows(&stderr);
    assert!(
        rows.contains(&("rust".to_string(), "call".to_string())),
        "no rust/call row in {stderr}"
    );
}

// The µs column is descending, so the table names the expensive leg first.
#[test]
fn summary_rows_sort_by_micros_descending() {
    let state = Arc::new(SummaryState::new());
    let subscriber = Registry::default().with(SummaryLayer::new(Arc::clone(&state)));
    let content = std::fs::read("tests/fixtures/rust/lib.rs").expect("read fixture");
    tracing::subscriber::with_default(subscriber, || {
        dispatch("tests/fixtures/rust/lib.rs", &content, FamilyMask::ALL)
            .expect("rust source matches");
    });
    let table = state.render();
    let micros: Vec<u128> = family_body(&table)
        .into_iter()
        .filter_map(|line| line.split_whitespace().nth(2)?.parse().ok())
        .collect();
    assert!(micros.len() > 2, "too few rows in\n{table}");
    assert!(
        micros.windows(2).all(|pair| pair[0] >= pair[1]),
        "rows are not wall descending in\n{table}"
    );
}

/// Per lang: how many full content hashes and how many parses ONE file costs.
/// Every lang hashes ONCE, the extract cache key in `dispatch.rs`; a door that
/// needs the id reads it back. failure-modes 107 owns the count.
const HASHES_PER_FILE: [(&str, u64); 3] = [("go", 1), ("ts", 1), ("rust", 1)];
const PARSES_PER_FILE: u64 = 2;

/// The chain phase's site count on `go_residual/callers.go`, hand-counted off
/// the phase table and pinned so a chain walk that doubles is a FAIL.
const GO_RESIDUAL_CHAIN_SITES: u64 = 10;

// SABOTAGE RECEIPT: drop the `extracting_blob` read at `go.rs:80` back to a bare
// `content_id_of`, or the `EXTRACTING` set in `dispatch.rs:63` that feeds it, and
// `go hash` reads 2 per file against the 1 below; the same at `ts.rs:1698` for ts.
// A second blake3 is linear in file size and no wall budget on a loaded machine
// separates it from the machine.
#[test]
fn phase_calls_per_file_are_pinned() {
    let corpus = [
        ("go", ["go/sample.go", "go/docs.go", "go/edges.go"]),
        ("ts", ["ts/sample.ts", "ts/docs.ts", "ts/consts.ts"]),
        ("rust", ["rust/sample.rs", "rust/docs.rs", "rust/lib.rs"]),
    ];
    for (lang, files) in corpus {
        let want_hashes = HASHES_PER_FILE
            .iter()
            .find_map(|(name, count)| (*name == lang).then_some(*count))
            .expect("every lang in the corpus is priced");
        for file in files {
            let table = phases_of(&format!("tests/fixtures/{file}"));
            let (hash_files, hash_calls, _) = phase_row(&table, lang, "hash")
                .unwrap_or_else(|| panic!("no {lang} hash row for {file} in\n{table}"));
            assert_eq!(
                (hash_files, hash_calls),
                (want_hashes, want_hashes),
                "{lang} hashed {file} {hash_files} times, want {want_hashes}\n{table}"
            );
            let (parse_files, _, _) = phase_row(&table, lang, "parse")
                .unwrap_or_else(|| panic!("no {lang} parse row for {file} in\n{table}"));
            assert_eq!(
                parse_files, PARSES_PER_FILE,
                "{lang} parsed {file} {parse_files} times\n{table}"
            );
            let (flatten_files, _, _) = phase_row(&table, "-", "flatten")
                .unwrap_or_else(|| panic!("no flatten row for {file} in\n{table}"));
            assert_eq!(flatten_files, 1, "{file} flattened {flatten_files} times");
        }
    }
    let table = phases_of("tests/fixtures/go_residual/callers.go");
    let (_, chain_calls, _) =
        phase_row(&table, "go", "chain").unwrap_or_else(|| panic!("no go chain row in\n{table}"));
    assert_eq!(
        chain_calls, GO_RESIDUAL_CHAIN_SITES,
        "the go chain walk entered {chain_calls} sites\n{table}"
    );
}

#[test]
fn the_phase_table_names_only_phases_that_ran() {
    let table = phases_of("tests/fixtures/rust/sample.rs");
    for phase in ["hash", "parse", "family", "tsi_syntax", "write"] {
        let lang = if matches!(phase, "write") {
            "-"
        } else {
            "rust"
        };
        assert!(
            phase_row(&table, lang, phase).is_some(),
            "no {lang}/{phase} row in\n{table}"
        );
    }
    // A rust file enters no go leg and no resolve leg, so neither may appear.
    for (lang, phase) in [("go", "bind_plan"), ("rust", "resolve_leg")] {
        assert!(
            phase_row(&table, lang, phase).is_none(),
            "{lang}/{phase} ran on a plain rust extract\n{table}"
        );
    }
}

// FAIL-FIRST RECEIPT: `--bench` printed one `eprintln!` line per file whose
// shape no test read, so the numbers were never comparable across runs.
#[test]
fn bench_reports_through_the_summary_table() {
    let output = Command::new(BIN)
        .args(["--bench", "--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env_remove("DL_TRACE_SUMMARY")
        .env("DL_TRAIL", "0")
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("extract summary: wall ") && stderr.contains("extract phases: load "),
        "--bench printed no summary, got {stderr}"
    );
    assert!(
        !stderr.contains(" serial "),
        "the old per-file bench line survives: {stderr}"
    );
}

#[test]
fn the_default_run_creates_no_file_under_home() {
    let home = std::env::temp_dir().join("sprefa-extract-31-silence");
    let _ = std::fs::remove_dir_all(&home);
    let output = Command::new(BIN)
        .args(["--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env_remove("DL_TRACE_SUMMARY")
        .env_remove("DL_TRAIL")
        .env("HOME", &home)
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    assert!(
        !home.exists(),
        "a default run wrote under {}",
        home.display()
    );
}
