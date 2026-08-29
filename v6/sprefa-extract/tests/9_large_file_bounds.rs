//! Resource bounds on one large file: peak RSS of the `extract` binary over a
//! synthetic 2 MB source, per family mask.
//!
//! FAIL-PRE-FIX RECEIPT, release, 2 MB inputs:
//!   js --family cst           300,564,480 B (143x) -> 94,470,144 B (45x)
//!   rs --family cst,type,...  604,618,752 B (288x) -> 289,734,656 B (138x)
//! `flatten` collected every row of a one-pass stream, then `extend` copied the
//! per-family vector into the whole-file one, so two full copies coexisted.
//!
//! The budgets are a MULTIPLE OF THE INPUT: what is left is the parse tree
//! (tree-sitter for js, tree-sitter + syn for rs), which scales with the input
//! and not with the row count. They sit ~24% over the measured post-fix ratio.
//!
//! 2 MB and not 5 MB: the debug profile the whole-crate gate runs takes 21 s on
//! a 5 MB `.rs`, which is over the 10-second law for one operation.
//!
//! macOS only: peak RSS is read from `/usr/bin/time -l`. A Linux port reads
//! `/usr/bin/time -v`'s "Maximum resident set size".

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

const EXTRACT: &str = env!("CARGO_BIN_EXE_extract");

/// Ordinary JavaScript: enough distinct identifiers that the string interner is
/// exercised, and a nesting depth that keeps the CST non-trivial.
fn generate_js(bytes: usize) -> String {
    let mut out = String::with_capacity(bytes + 4096);
    let mut n = 0usize;
    while out.len() < bytes {
        out.push_str(&format!(
            "function handler_{n}(alpha_{n}, beta_{n}) {{\n  const gamma_{n} = alpha_{n} + beta_{n};\n  if (gamma_{n} > {n}) {{\n    return {{ label: \"row_{n}\", value: gamma_{n} }};\n  }}\n  return helper_{n}(gamma_{n}).then((delta_{n}) => delta_{n} * 2);\n}}\n"
        ));
        n += 1;
    }
    out
}

/// Ordinary Rust in the shape the finding's input has: one very long run of
/// small generated items.
fn generate_rs(bytes: usize) -> String {
    let mut out = String::with_capacity(bytes + 4096);
    let mut n = 0usize;
    while out.len() < bytes {
        out.push_str(&format!(
            "pub fn handler_{n}(alpha_{n}: u32, beta_{n}: u32) -> Option<u32> {{\n    let gamma_{n} = alpha_{n} + beta_{n};\n    if gamma_{n} > {n} {{\n        return Some(gamma_{n});\n    }}\n    helper_{n}(gamma_{n}).map(|delta_{n}| delta_{n} * 2)\n}}\n"
        ));
        n += 1;
    }
    out
}

fn scratch(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sprefa-extract-large-bounds");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("scratch write");
    path
}

/// Peak RSS in bytes plus the stdout line count of one `extract` run, measured
/// by `/usr/bin/time -l` around the real binary. Measuring the child is the
/// point: an in-process measurement would miss the binary's own buffering.
fn run_measured(args: &[&str]) -> (u64, usize) {
    let mut command = Command::new("/usr/bin/time");
    command.arg("-l").arg(EXTRACT).args(args);
    let out = command.output().expect("spawn /usr/bin/time");
    assert!(
        out.status.success(),
        "extract {args:?} rc={:?}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let rss = stderr
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_suffix("maximum resident set size")
                .map(|value| value.trim().parse::<u64>().expect("rss is a number"))
        })
        .unwrap_or_else(|| panic!("no rss line in:\n{stderr}"));
    let lines = out.stdout.iter().filter(|byte| **byte == b'\n').count();
    (rss, lines)
}

/// `--bench` reports the flattened fact count on stderr. Streaming the rows
/// must not change how many there are, so the streamed stdout line count and
/// this number are asserted equal.
fn bench_facts(args: &[&str]) -> usize {
    let out = Command::new(EXTRACT)
        .arg("--bench")
        .args(args)
        .output()
        .expect("spawn extract --bench");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let marker = stderr
        .rsplit_once("facts=")
        .unwrap_or_else(|| panic!("no facts= in:\n{stderr}"))
        .1;
    marker
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .expect("facts= is a number")
}

fn assert_bounded(path: &Path, family: &str, rss_per_input_byte: u64) {
    let input = std::fs::metadata(path).expect("scratch metadata").len();
    let path = path.to_string_lossy().to_string();
    let (rss, lines) = run_measured(&["--family", family, &path]);
    let budget = input * rss_per_input_byte;
    assert!(
        rss <= budget,
        "{path} --family {family}: peak RSS {rss} B over the {budget} B budget \
         ({rss_per_input_byte}x the {input} B input); {lines} rows streamed"
    );
    let facts = bench_facts(&["--family", family, &path]);
    assert_eq!(
        lines, facts,
        "{path} --family {family}: streamed {lines} rows, --bench counted {facts}"
    );
}

/// Measured floor 45x (the tree-sitter tree alone); 56x leaves no room for a
/// second full copy of the row stream, which cost 143x.
#[test]
fn js_cst_rss_is_bounded() {
    let path = scratch("bounds.js", &generate_js(2 * 1024 * 1024));
    assert_bounded(&path, "cst", 56);
}

/// The rust arm parses with syn AND tree-sitter, so its floor is 138x against
/// the js arm's 45x. 172x still excludes the 288x materialized vector.
#[test]
fn rs_all_families_rss_is_bounded() {
    let path = scratch("bounds.rs", &generate_rs(2 * 1024 * 1024));
    assert_bounded(&path, "cst,type,call,df", 172);
}
