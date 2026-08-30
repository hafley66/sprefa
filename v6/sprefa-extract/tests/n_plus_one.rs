// TEST: the resolve paths that walked one collection per member of another.
// Each case builds its own fixture at N >= 1000, so a return to the scan shape
// is quadratic and cannot also stay under 2s at these sizes.

use sprefa_extract::{
    OccurrenceRole, PositionEncoding, ScipDocument, ScipIndex, ScipOccurrence, ScipSignature,
    ScipSymbolInfo, SymbolInterner,
};

/// One occurrence per line, each naming the identifier at column 0.
fn document(lines: usize) -> (ScipDocument, Vec<u8>, Vec<String>) {
    let mut content = String::new();
    let mut occurrences = Vec::with_capacity(lines);
    let mut syms = SymbolInterner::default();
    for line in 0..lines {
        content.push_str("callee(argument, other);\n");
        occurrences.push(ScipOccurrence {
            symbol: syms.intern(format!("scip crate . symbol{line}#")),
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
            ..ScipDocument::default()
        },
        bytes,
        syms.table(),
    )
}

// TEST: one call site reads the document's bytes once, then answers each
// occurrence's range off the line table cached on the document. Pre-fix
// `byte_range` walked the document from offset 0 for every occurrence, twice
// (start and end), so 4000 occurrences over a 100000-byte document read 800
// million bytes; a return to the scan shape is quadratic and cannot also stay
// under 2s at these sizes.
#[test]
fn a_call_site_reads_the_document_once_not_once_per_occurrence() {
    let lines = 4_000usize;
    let (doc, content, _symbols) = document(lines);
    let site = sprefa_extract::Span { start: 0, len: 6 };
    let started = std::time::Instant::now();
    let hit = sprefa_extract::site_occurrence(&doc, &content, site, "callee");
    assert!(
        hit.is_some(),
        "the site's own occurrence is in the document"
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
    let (doc, content, symbols) = document(lines);
    let index = sprefa_extract::ScipIndex {
        metadata: Default::default(),
        documents: vec![doc],
        external_symbols: Vec::new(),
        symbols,
        ..ScipIndex::default()
    };
    let reader = |_path: &str| -> Option<Vec<u8>> { Some(content.clone()) };
    let started = std::time::Instant::now();
    let rows = sprefa_extract::flatten_scip_records(
        &index,
        &reader,
        &sprefa_extract::ScipRecords::default(),
        false,
    );
    assert!(
        rows.len() >= lines,
        "{} rows for {lines} occurrences",
        rows.len()
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
    let mut syms = SymbolInterner::default();
    for line in 0..occurrences {
        text.push_str("field: Type,\n");
        signature_occurrences.push(ScipOccurrence {
            symbol: syms.intern(format!("scip crate . type{line}#")),
            range: [line as i32, 7, line as i32, 11],
            roles: OccurrenceRole(0),
            syntax_kind: 0,
            enclosing_range: None,
            override_documentation: Vec::new(),
            diagnostics: Vec::new(),
        });
    }
    let info = ScipSymbolInfo {
        symbol: syms.intern("scip crate . Owner#"),
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
            ..ScipDocument::default()
        }],
        external_symbols: Vec::new(),
        symbols: syms.table(),
        ..ScipIndex::default()
    };
    let reader = |_path: &str| -> Option<Vec<u8>> { None };
    let started = std::time::Instant::now();
    let rows = sprefa_extract::flatten_scip_records(
        &index,
        &reader,
        &sprefa_extract::ScipRecords::default(),
        false,
    );
    assert!(rows.len() >= occurrences, "{} rows", rows.len());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}
