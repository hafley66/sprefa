//! `rust_mbe::expand_file` plus the `RustSource::extract` call-arm hook that
//! splices it in. `RustSource.extract(fixture)` now folds the gained facts
//! straight into `call.nodes`/`call.aux.sites`, spans already mapped back to
//! the original file, so these end-to-end counts ARE the lab's "expanded"
//! column (`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 1 table).

use sprefa_extract::lang::rust_mbe::expand_file;
use sprefa_extract::{FamilyMask, RustSource, Source};

const MBE_DIR: &str = "tests/fixtures/rust_findings/mbe";

fn counts(content: &str) -> (usize, usize) {
    let output = RustSource.extract("f.rs", content.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().expect("syn parse must succeed");
    (call.nodes.len(), call.aux.sites.len())
}

fn read(name: &str) -> String {
    std::fs::read_to_string(format!("{MBE_DIR}/{name}")).expect(name)
}

/// SABOTAGE RECEIPT: dropping the `collect_calls` name filter (matching every
/// `MacroCall` regardless of whether a local def exists) makes f2/f4/f5/f8
/// fail their `is_none()` check, since `include!`/attribute macros would then
/// be mistaken for local invocations.
#[test]
fn no_local_macro_leaves_file_untouched() {
    for name in [
        "f2_cross_file.rs",
        "f4_builtins.rs",
        "f5_derive.rs",
        "f6_attr_proc_macro.rs",
        "f8_include.rs",
    ] {
        let src = read(name);
        assert!(expand_file(&src).is_none(), "{name} should stay unexpanded");
    }
}

#[test]
fn f1_local_call_in_body_gains_two_sites() {
    let src = read("f1_local_call_in_body.rs");
    assert!(!expand_file(&src).unwrap().budget_hit);
    assert_eq!(counts(&src), (2, 2));
}

#[test]
fn f3_nested_invocations_settle_to_one_site() {
    let src = read("f3_nested.rs");
    assert!(!expand_file(&src).unwrap().budget_hit);
    assert_eq!(counts(&src), (2, 1));
}

#[test]
fn f7_mints_fn_gains_a_def_and_a_site() {
    let src = read("f7_mints_fn.rs");
    assert!(!expand_file(&src).unwrap().budget_hit);
    assert_eq!(counts(&src), (3, 2));
}

/// Every def/site gained by expansion reports the ORIGINAL invocation's span,
/// never a spliced-text offset with no home in the source file.
#[test]
fn gained_site_spans_map_inside_the_invocation() {
    let src = read("f7_mints_fn.rs");
    let expanded = expand_file(&src).expect("mkfn! is local");
    let output = RustSource.extract("f.rs", expanded.text.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let mut saw_macro_site = false;
    for site in &call.aux.sites {
        let range = site.span.start..site.span.start + site.span.len;
        if expanded.is_macro_span(range.clone()) {
            saw_macro_site = true;
            let original = expanded.map_span(range).expect("macro span always maps");
            let text = &src[original.start as usize..(original.start + original.len) as usize];
            assert!(
                text.contains("mkfn"),
                "mapped span {original:?} does not cover the mkfn! invocation: {text:?}"
            );
        }
    }
    assert!(
        saw_macro_site,
        "f7's generated() call should be macro-origin"
    );
}

/// The `RustSource::extract` hook maps a gained site's span all the way to
/// ORIGINAL file coordinates: a caller reading the normal wire never sees a
/// spliced-text offset.
#[test]
fn extract_reports_the_gained_site_at_its_invocation_in_the_source_file() {
    let src = read("f7_mints_fn.rs");
    let mkfn_start = src.find("mkfn!").expect("fixture invokes mkfn!") as u32;
    let mkfn_end = mkfn_start + src[mkfn_start as usize..].find('}').unwrap() as u32 + 1;

    let output = RustSource.extract("f.rs", src.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();
    let inner_call_site = call
        .aux
        .sites
        .iter()
        .find(|s| output.strings.lookup(s.callee) == "inner_call")
        .expect("mkfn!'s expansion calls inner_call()");

    assert!(inner_call_site.span.start >= mkfn_start && inner_call_site.span.end() <= mkfn_end);
}

/// The wire's `macro_site` row (`CallFAux.macro_sites`) names the invocation
/// that minted the gained facts, tagged `source: mbe`.
#[test]
fn extract_emits_one_macro_site_row_naming_mkfn() {
    let src = read("f7_mints_fn.rs");
    let output = RustSource.extract("f.rs", src.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    assert_eq!(call.aux.macro_sites.len(), 1);
    let site = &call.aux.macro_sites[0];
    assert_eq!(output.strings.lookup(site.macro_name), "mkfn");
    assert_eq!(site.source, sprefa_extract::types::MacroSiteSource::Mbe);
    let text = &src[site.span.start as usize..(site.span.start + site.span.len) as usize];
    assert!(text.contains("mkfn"));
}

/// A macro that keeps re-minting itself never terminates a fixpoint; the pass
/// cap is what stops it, not a growing byte count.
#[test]
fn recursive_macro_hits_the_pass_budget() {
    let src = read("f9_recursive.rs");
    let expanded = expand_file(&src).expect("spin! is local");
    assert!(expanded.budget_hit);
}

fn rs_files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// COUNT: `expand_file` is one `ra_ap_syntax` parse per file (plus a second
/// parse per pass that still finds an invocation); ignored by default since it
/// needs a local rust-analyzer checkout. Also the mbe.macro_sites.tsv receipt
/// for `plans/extract-crawl-2026-08-29/rust.REPORT.md` section 14.
#[test]
#[ignore]
fn corpus_wall_time_and_macro_sites_tsv() {
    let corpus = std::path::Path::new("/Users/chrishafley/projects/rust-analyzer/crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(corpus).expect("rust-analyzer checkout at this path") {
        let crate_dir = entry.unwrap().path().join("src");
        rs_files_under(&crate_dir, &mut files);
    }
    files.sort();

    let t0 = std::time::Instant::now();
    let mut tsv = String::from("path\tstart\tend\tmacro_name\n");
    let mut files_with_macros = 0usize;
    let mut budget_hits = 0usize;
    for path in &files {
        let content = std::fs::read_to_string(path).unwrap();
        let Some(expanded) = expand_file(&content) else {
            continue;
        };
        files_with_macros += 1;
        if expanded.budget_hit {
            budget_hits += 1;
            eprintln!("budget_hit: {}", path.display());
        }
        let rel = path
            .strip_prefix("/Users/chrishafley/projects/rust-analyzer/")
            .unwrap();
        for (span, name) in expanded.macro_sites() {
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                rel.display(),
                span.start,
                span.end(),
                name
            ));
        }
    }
    let wall = t0.elapsed();

    std::fs::write(
        "../../plans/extract-macro-lab-2026-08-29/mbe.macro_sites.tsv",
        &tsv,
    )
    .expect("plans dir is writable from the crate root");

    eprintln!(
        "files={} with_macros={} budget_hits={} wall_ms={}",
        files.len(),
        files_with_macros,
        budget_hits,
        wall.as_millis()
    );
    assert!(
        wall.as_secs() < 2,
        "wall {wall:?} exceeds the 2s COUNT budget"
    );
}
