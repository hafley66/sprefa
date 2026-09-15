//! The book in `book/`, through the real binary. Every `dl7` block in
//! `book/src/*.md` opens with `; fixture: v8/<path>[:first-last]`, then
//! optionally `; diagnostic: <name>` and `; compile: <flags>`. `dl8 compile`
//! on the whole named file must exit 0, or exit nonzero with that diagnostic;
//! a block under `oracle/check/cases` must name one. `book/check_blocks.sh`
//! diffs every block against its file, `book/check_outputs.sh` reruns every
//! `console` block with `DL8` set to this binary, and `mdbook build` renders
//! the book. Every child is killed past 10 s.

use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUN_CAP: Duration = Duration::from_secs(10);

/// Children at once; the checks run in parallel without taking every core.
const LANES: usize = 2;

fn v8() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn chapters() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(v8().join("book/src"))
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|x| x == "md"))
        .collect();
    out.sort();
    out
}

struct MarkedBlock {
    chapter: String,
    path: String,
    diagnostic: Option<String>,
    compile_flags: Vec<String>,
}

fn marked_blocks(chapter: &Path) -> Vec<MarkedBlock> {
    let name = chapter.file_name().unwrap().to_string_lossy().to_string();
    let text = std::fs::read_to_string(chapter).unwrap();
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line != "```dl7" {
            continue;
        }
        let marker = lines.next().unwrap_or_default();
        let spec = marker
            .strip_prefix("; fixture: v8/")
            .unwrap_or_else(|| panic!("{name}: dl7 block without a v8 fixture marker: {marker}"));
        let path = spec.split(':').next().unwrap().to_string();
        let mut block = MarkedBlock {
            chapter: name.clone(),
            path,
            diagnostic: None,
            compile_flags: Vec::new(),
        };
        for directive in lines.by_ref() {
            if let Some(diagnostic) = directive.strip_prefix("; diagnostic: ") {
                block.diagnostic = Some(diagnostic.to_string());
            } else if let Some(flags) = directive.strip_prefix("; compile: ") {
                block.compile_flags = flags.split_whitespace().map(String::from).collect();
            } else {
                break;
            }
        }
        for rest in lines.by_ref() {
            if rest == "```" {
                break;
            }
        }
        out.push(block);
    }
    out
}

/// Exit code, stdout and stderr of one child, killed past `RUN_CAP`.
fn finish(command: &mut Command) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    let mut stdout = child.stdout.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let mut stderr = child.stderr.take().unwrap();
    let stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() > RUN_CAP {
            let _ = child.kill();
            return Err(format!("exceeded {RUN_CAP:?}"));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    Ok((
        status.code().unwrap_or(-1),
        stdout.join().unwrap(),
        stderr.join().unwrap(),
    ))
}

/// A diagnostic is `diagnostic(Phase, Location, Payload)`; the payload's
/// functor, or its atom, is the name.
fn diagnostic_names(compiled: &Value) -> Vec<String> {
    compiled["diagnostics"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|d| {
                    let payload = &d["args"][2];
                    payload["f"]
                        .as_str()
                        .or(payload["a"].as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn check_block(block: &MarkedBlock) -> Result<(), String> {
    let label = format!("{}: {}", block.chapter, block.path);
    let expects_diagnostic = block.path.starts_with("oracle/check/cases/");
    if expects_diagnostic && block.diagnostic.is_none() {
        return Err(format!("{label}: a check case block names no diagnostic"));
    }
    let (code, stdout, stderr) = finish(
        Command::new(env!("CARGO_BIN_EXE_dl8"))
            .current_dir(v8())
            .arg("compile")
            .arg(&block.path)
            .args(&block.compile_flags),
    )
    .map_err(|e| format!("{label}: {e}"))?;
    let compiled: Value = serde_json::from_slice(&stdout).map_err(|e| {
        format!(
            "{label}: no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&stderr)
        )
    })?;
    let names = diagnostic_names(&compiled);
    match &block.diagnostic {
        None if code == 0 => Ok(()),
        None => Err(format!("{label}: exit {code}, diagnostics {names:?}")),
        Some(_) if code == 0 => Err(format!("{label}: exit 0, a diagnostic was named")),
        Some(named) if names.contains(named) => Ok(()),
        Some(named) => Err(format!("{label}: no {named} in {names:?}")),
    }
}

fn in_lanes<T: Sync, F: Fn(&T) -> Result<(), String> + Sync>(items: &[T], check: F) -> Vec<String> {
    let chunk = items.len().div_ceil(LANES).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|lane| {
                scope.spawn(|| {
                    lane.iter()
                        .filter_map(|item| check(item).err())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect()
    })
}

#[test]
pub fn every_block_equals_its_fixture() {
    let (code, stdout, stderr) =
        finish(Command::new("bash").arg(v8().join("book/check_blocks.sh"))).unwrap();
    assert_eq!(
        code,
        0,
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    print!("{}", String::from_utf8_lossy(&stdout));
}

/// One chapter: its marked blocks compile to what they name, in lanes, then
/// its console blocks rerun through `book/check_outputs.sh`.
fn chapter_holds(stem: &str) {
    let chapter = v8().join("book/src").join(format!("{stem}.md"));
    let blocks = marked_blocks(&chapter);
    let mut failures = in_lanes(&blocks, check_block);
    let outputs = finish(
        Command::new("bash")
            .arg(v8().join("book/check_outputs.sh"))
            .arg(&chapter)
            .env("DL8", env!("CARGO_BIN_EXE_dl8")),
    );
    match outputs {
        Ok((0, stdout, _)) => print!("{stem}: {}", String::from_utf8_lossy(&stdout)),
        Ok((_, stdout, stderr)) => failures.push(format!(
            "{}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        )),
        Err(e) => failures.push(format!("{stem} outputs: {e}")),
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    println!(
        "{stem}: {} marked blocks compile to what they name",
        blocks.len()
    );
}

/// A chapter with no test below would pass unchecked.
const CHAPTERS: [&str; 17] = [
    "0_why",
    "1_run",
    "2_declare",
    "3_rules",
    "4_negation",
    "5_terms",
    "6_compare",
    "7_aggregate",
    "8_macros",
    "9_modules",
    "10_effects",
    "11_executors",
    "12_store",
    "13_sqlite",
    "14_diagnostics",
    "15_demos",
    "16_not_built",
];

#[test]
pub fn every_chapter_has_a_test() {
    let found: Vec<String> = chapters()
        .iter()
        .map(|path| path.file_stem().unwrap().to_string_lossy().to_string())
        .filter(|stem| stem != "SUMMARY")
        .collect();
    let mut listed: Vec<String> = CHAPTERS.iter().map(|stem| stem.to_string()).collect();
    listed.sort();
    assert_eq!(found, listed);
}

#[test]
pub fn chapter_why() {
    chapter_holds("0_why");
}

#[test]
pub fn chapter_run() {
    chapter_holds("1_run");
}

#[test]
pub fn chapter_declare() {
    chapter_holds("2_declare");
}

#[test]
pub fn chapter_rules() {
    chapter_holds("3_rules");
}

#[test]
pub fn chapter_negation() {
    chapter_holds("4_negation");
}

#[test]
pub fn chapter_terms() {
    chapter_holds("5_terms");
}

#[test]
pub fn chapter_compare() {
    chapter_holds("6_compare");
}

#[test]
pub fn chapter_aggregate() {
    chapter_holds("7_aggregate");
}

#[test]
pub fn chapter_macros() {
    chapter_holds("8_macros");
}

#[test]
pub fn chapter_modules() {
    chapter_holds("9_modules");
}

#[test]
pub fn chapter_effects() {
    chapter_holds("10_effects");
}

#[test]
pub fn chapter_executors() {
    chapter_holds("11_executors");
}

#[test]
pub fn chapter_store() {
    chapter_holds("12_store");
}

#[test]
pub fn chapter_sqlite() {
    chapter_holds("13_sqlite");
}

#[test]
pub fn chapter_diagnostics() {
    chapter_holds("14_diagnostics");
}

#[test]
pub fn chapter_demos() {
    chapter_holds("15_demos");
}

#[test]
pub fn chapter_not_built() {
    chapter_holds("16_not_built");
}

#[test]
pub fn mdbook_builds_the_book() {
    let (code, stdout, stderr) = finish(
        Command::new("mdbook")
            .current_dir(v8())
            .args(["build", "book"]),
    )
    .expect("mdbook on PATH: cargo install mdbook --locked");
    assert_eq!(
        code,
        0,
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        v8().join("book/book/index.html").exists(),
        "no book/book/index.html"
    );
}
