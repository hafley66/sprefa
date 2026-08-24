//! The tracing seam: stderr silence by default, and the summary layer's table.
#![cfg(feature = "cli")]

use std::process::Command;
use std::sync::Arc;

use sprefa_extract::trace::{SummaryLayer, SummaryState};
use sprefa_extract::{dispatch, FamilyMask};
use tracing_subscriber::{layer::SubscriberExt, Registry};

const BIN: &str = env!("CARGO_BIN_EXE_extract");
const FIXTURE: &str = "tests/fixtures/rust/sample.rs";

/// The (lang, family) pairs the rendered table names, header row dropped.
fn table_rows(table: &str) -> Vec<(String, String)> {
    table
        .lines()
        .skip(2)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_string(), fields.next()?.to_string()))
        })
        .collect()
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
    let micros: Vec<u128> = table
        .lines()
        .skip(2)
        .filter_map(|line| line.split_whitespace().nth(2)?.parse().ok())
        .collect();
    assert!(micros.len() > 2, "too few rows in\n{table}");
    assert!(
        micros.windows(2).all(|pair| pair[0] >= pair[1]),
        "rows are not wall descending in\n{table}"
    );
}
