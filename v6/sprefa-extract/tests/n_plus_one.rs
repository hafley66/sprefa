// TEST: the resolve paths that walked one collection per member of another.
// Each case builds its own fixture at N >= 1000 and reads a probe counter the
// crate arms, so a return to the scan shape is a count. The elapsed assertions
// are the second gate: a quadratic that stays under the count bound cannot also
// stay under 2s at these sizes.

use sprefa_extract::{
    OccurrenceRole, PositionEncoding, ScipDocument, ScipOccurrence, ScipSignature, ScipSymbolInfo,
};

/// The probe counters are process-wide, so two cases reading them at once
/// would each see the other's arithmetic.
static PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// One occurrence per line, each naming the identifier at column 0.
fn document(lines: usize) -> (ScipDocument, Vec<u8>) {
    let mut content = String::new();
    let mut occurrences = Vec::with_capacity(lines);
    for line in 0..lines {
        content.push_str("callee(argument, other);\n");
        occurrences.push(ScipOccurrence {
            symbol: format!("scip crate . symbol{line}#"),
            range: [line as i32, 0, line as i32, 6],
            roles: OccurrenceRole(0),
            syntax_kind: 0,
            enclosing_range: None,
            override_documentation: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    let bytes = content.into_bytes();
    (
        ScipDocument {
            relative_path: "measured.rs".to_string(),
            position_encoding: PositionEncoding::Utf8,
            occurrences,
            symbols: Vec::new(),
            language: "Rust".to_string(),
            text: String::new(),
        },
        bytes,
    )
}

// TEST: one call site reads the document's bytes once, then answers each
// occurrence's range off the line table. Pre-fix `byte_range` walked the
// document from offset 0 for every occurrence, twice (start and end), so 4000
// occurrences over a 100000-byte document read 800 million bytes and this
// assertion read 108001 vs 800104000.
#[test]
fn a_call_site_reads_the_document_once_not_once_per_occurrence() {
    let lines = 4_000usize;
    let (doc, content) = document(lines);
    let site = sprefa_extract::Span { start: 0, len: 6 };
    let _serial = PROBE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_extract::scip::line_reads();
    let started = std::time::Instant::now();
    let hit = sprefa_extract::site_occurrence(&doc, &content, site, "callee");
    let reads = sprefa_extract::scip::line_reads() - before;
    assert!(
        hit.is_some(),
        "the site's own occurrence is in the document"
    );
    assert!(
        reads <= content.len() as u64 + 4 * lines as u64,
        "{reads} document reads for {lines} occurrences over {} bytes is a rescan",
        content.len()
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: flattening a document's occurrence rows reads the bytes once for the
// whole document. Pre-fix each occurrence rescanned from offset 0 and this
// assertion read 108001 vs 800104000.
#[test]
fn flattening_occurrences_reads_the_document_once() {
    let lines = 4_000usize;
    let (doc, content) = document(lines);
    let index = sprefa_extract::ScipIndex {
        metadata: Default::default(),
        documents: vec![doc],
        external_symbols: Vec::new(),
    };
    let reader = |_path: &str| -> Option<Vec<u8>> { Some(content.clone()) };
    let _serial = PROBE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_extract::scip::line_reads();
    let started = std::time::Instant::now();
    let rows = sprefa_extract::flatten_scip_records(
        &index,
        &reader,
        &sprefa_extract::ScipRecords::default(),
        false,
    );
    let reads = sprefa_extract::scip::line_reads() - before;
    assert!(
        rows.len() >= lines,
        "{} rows for {lines} occurrences",
        rows.len()
    );
    assert!(
        reads <= content.len() as u64 + 4 * lines as u64,
        "{reads} document reads for {lines} occurrences over {} bytes is a rescan",
        content.len()
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: a signature's occurrence ranges convert off one table for the whole
// signature text. Pre-fix each occurrence rescanned the signature and this
// assertion read 24001 vs 48000000.
#[test]
fn signature_occurrences_read_the_signature_once() {
    let occurrences = 2_000usize;
    let mut text = String::new();
    let mut signature_occurrences = Vec::with_capacity(occurrences);
    for line in 0..occurrences {
        text.push_str("field: Type,\n");
        signature_occurrences.push(ScipOccurrence {
            symbol: format!("scip crate . type{line}#"),
            range: [line as i32, 7, line as i32, 11],
            roles: OccurrenceRole(0),
            syntax_kind: 0,
            enclosing_range: None,
            override_documentation: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    let bytes = text.len() as u64;
    let info = ScipSymbolInfo {
        symbol: "scip crate . Owner#".to_string(),
        display_name: "Owner".to_string(),
        kind: 0,
        relationships: Vec::new(),
        documentation: Vec::new(),
        signature: Some(ScipSignature {
            language: "Rust".to_string(),
            text,
            occurrences: signature_occurrences,
        }),
        enclosing_symbol: String::new(),
    };
    let index = sprefa_extract::ScipIndex {
        metadata: Default::default(),
        documents: vec![ScipDocument {
            relative_path: "measured.rs".to_string(),
            position_encoding: PositionEncoding::Utf8,
            occurrences: Vec::new(),
            symbols: vec![info],
            language: "Rust".to_string(),
            text: String::new(),
        }],
        external_symbols: Vec::new(),
    };
    let reader = |_path: &str| -> Option<Vec<u8>> { None };
    let _serial = PROBE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_extract::scip::line_reads();
    let started = std::time::Instant::now();
    let rows = sprefa_extract::flatten_scip_records(
        &index,
        &reader,
        &sprefa_extract::ScipRecords::default(),
        false,
    );
    let reads = sprefa_extract::scip::line_reads() - before;
    assert!(rows.len() >= occurrences, "{} rows", rows.len());
    assert!(
        reads <= bytes + 4 * occurrences as u64,
        "{reads} signature reads for {occurrences} occurrences over {bytes} bytes is a rescan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}
