//! THE TWO NAMED FAMILIES: `--family scip` and `--family diet_scip`.
//!
//! DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA. The
//! discriminating test below is the one that earns the two names: over the same
//! three files, `diet_scip` emits NOTHING and `scip` binds the call, because two
//! files define `helper` and only a real index knows which one the import means.
//!
//! BOTH GOLDENS PIN JSONL FIELD NAMES. The v6 host decodes by top-level key, so
//! a rename is a breaking change and has to show up as a diff. The `scip` golden
//! additionally pins the v5 relation vocabulary: `scip_def`, `scip_name`,
//! `scip_ref`, `scip_edge`, `scip_fn_edge`, `scip_callee_type`, `scip_local`,
//! `scip_impl`. A program written against v5's `scip_*` relations reads these
//! rows unchanged, and that is the whole contract.
//!
//! THE SCIP TESTS RUN THE REAL INDEXERS, matching the ratchet law in
//! golden_parity.rs: a missing indexer fails loudly rather than skipping to a
//! green that means nothing. rust-analyzer and scip-typescript are both expected
//! on PATH. The one place a MISSING indexer is exercised is the named-skip test,
//! which plants an empty PATH on purpose.
//!
//! EVERY SCIP RUN PASSES `--scip-cache` INTO A TEMP DIR. The default cache is
//! `<root>/.dl/.state`, and these roots are committed fixtures: a test must
//! never write into one, and a per-test cache also keeps the reuse test honest
//! about which run built and which reused.

use std::path::PathBuf;
use std::process::{Command, Output};

const SCIP_REL_ROOT: &str = "tests/fixtures/scip_rel";
const TS_ROOT: &str = "tests/fixtures/ts";
const RUST_ROOT: &str = "tests/fixtures/rust";
const SCIP_REL_GOLDEN: &str = include_str!("fixtures/scip_families/scip_rel.jsonl");
const DIET_SCIP_GOLDEN: &str = include_str!("fixtures/scip_families/diet_scip_ts.jsonl");

/// The three ts files that make the corpus-wide name ambiguous: alpha and beta
/// both export `helper`, gamma imports alpha's and calls it.
const TS_TRIO: [&str; 3] = [
    "tests/fixtures/ts/scip/alpha.ts",
    "tests/fixtures/ts/scip/beta.ts",
    "tests/fixtures/ts/scip/gamma.ts",
];

/// A fresh temp dir, named for the test that asked. No tempfile dep here, the
/// same reason `scip.rs` has none.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprefa-scip-family-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn raw(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn run(args: &[&str]) -> String {
    let output = raw(args);
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// `--family scip ROOT` with a private cache.
fn scip_family(root: &str, cache: &PathBuf, extra: &[&str]) -> String {
    let cache = cache.to_string_lossy().to_string();
    let mut args: Vec<&str> = vec!["--family", "scip", "--scip-cache", &cache];
    args.extend_from_slice(extra);
    args.push(root);
    run(&args)
}

fn records<'a>(stream: &'a str, kind: &str) -> Vec<&'a str> {
    let tag = format!("{{\"record\":\"{kind}\",");
    stream
        .lines()
        .filter(|line| line.starts_with(&tag))
        .collect()
}

// ════════════════════════════════════════════════════════════════════════════
// THE DISCRIMINATING RECEIPT
// ════════════════════════════════════════════════════════════════════════════

/// THE RECEIPT THAT JUSTIFIES TWO NAMES.
///
/// `scip/delta.ts` declares `Near.probe` and `scip/epsilon.ts` declares
/// `Far.probe`. `delta.ts` calls BOTH through typed receivers, importing
/// neither name. That second call is the whole test:
///
///   diet_scip  has two corpus definitions named `probe` and no import binding
///              for the name, so the module plane has no word and the name
///              match takes the same-file one. It cannot type a receiver.
///   scip       moves it to `epsilon.ts`, because scip-typescript ran the
///              TypeScript compiler over the program and the compiler knows.
///
/// This is one worked case of a whole class: any reference whose target is
/// chosen by the RECEIVER'S TYPE. The module plane closed the neighbouring
/// class (an unqualified name that IS imported: `gamma.ts` binds `helper` to
/// `alpha.ts` with no indexer at all), which is why this receipt moved here.
#[test]
fn only_real_scip_resolves_the_cross_file_call_the_heuristic_cannot() {
    let cache = scratch("discriminating");

    // The heuristic half: `near.probe()` binds the same-file `Near.probe`
    // through the receiver leg; `far.probe()` stays UNBOUND (a name match
    // cannot type `far`, so the site drops reason=inferred) until scip runs.
    let heuristic = run(&[
        "--family",
        "diet_scip",
        "tests/fixtures/ts/scip/delta.ts",
        "tests/fixtures/ts/scip/epsilon.ts",
    ]);
    let probe_edges = heuristic
        .lines()
        .filter(|line| line.contains(r#""record":"resolved_edge""#))
        .filter(|line| line.contains(r#""callee_name":"probe""#))
        .collect::<Vec<_>>();
    assert_eq!(probe_edges.len(), 1, "only the typed receiver binds: {heuristic}");
    assert!(
        probe_edges[0].contains(r#""callee_path":"tests/fixtures/ts/scip/delta.ts""#),
        "the receiver leg takes the same-file probe: {heuristic}"
    );
    assert!(
        heuristic
            .lines()
            .any(|line| line.contains(r#""reason":"inferred""#) && line.contains(r#""detail":"probe""#)),
        "the untyped far site drops reason=inferred: {heuristic}"
    );

    // The module plane's own half, on the neighbouring shape: an IMPORTED name
    // binds with no indexer, which is what stopped being scip's alone.
    let mut plane: Vec<&str> = vec!["--family", "diet_scip"];
    plane.extend_from_slice(&TS_TRIO);
    let bound = run(&plane);
    assert!(
        bound.contains(r#""callee_path":"tests/fixtures/ts/scip/alpha.ts""#)
            && bound.contains(r#""kind":"import_resolve""#),
        "an imported name binds through the module plane: {bound}"
    );

    // The real half: the call binds, and it binds to ALPHA.
    let real = scip_family(TS_ROOT, &cache, &[]);
    let edge = r#"{"record":"scip_fn_edge","caller":"scip-typescript npm . . scip/`gamma.ts`/use().","callee":"scip-typescript npm . . scip/`alpha.ts`/helper()."}"#;
    assert!(
        real.lines().any(|line| line == edge),
        "the compiler-resolved call edge must be present: {edge}"
    );
    let reference = r#"{"record":"scip_ref","file":"scip/gamma.ts","symbol":"scip-typescript npm . . scip/`alpha.ts`/helper().","def_file":"scip/alpha.ts","repo":"ts"}"#;
    assert!(
        real.lines().any(|line| line == reference),
        "the reference must name alpha as the defining file: {reference}"
    );

    // And it must NOT bind to beta. beta's `helper` is DEFINED, so a scip_def
    // row for it is correct and expected; what must not exist is any reference
    // or call edge reaching it, because nothing imports beta. An edge there
    // would mean the projection joined on names after all, which is the failure
    // this whole family exists to avoid.
    let beta_defs = real
        .lines()
        .filter(|line| line.contains("`beta.ts`/helper()."))
        .collect::<Vec<_>>();
    assert_eq!(
        beta_defs.len(),
        2,
        "beta's helper is defined and named, and nothing else: {beta_defs:?}"
    );
    assert!(
        beta_defs
            .iter()
            .all(|line| line.starts_with("{\"record\":\"scip_def\"")
                || line.starts_with("{\"record\":\"scip_name\"")),
        "no reference or call edge may reach beta: {beta_defs:?}"
    );
}

/// The same discrimination in Rust, through a different indexer, so the result
/// is a property of having a real index rather than a quirk of one tool.
/// `scip/gamma.rs` does `use crate::scip::alpha::helper` and calls it; alpha and
/// beta both define `helper`.
#[test]
fn the_discrimination_holds_through_rust_analyzer_too() {
    let cache = scratch("discriminating-rust");

    let mut diet: Vec<&str> = vec!["--family", "diet_scip"];
    let trio = [
        "tests/fixtures/rust/scip/alpha.rs",
        "tests/fixtures/rust/scip/beta.rs",
        "tests/fixtures/rust/scip/gamma.rs",
    ];
    diet.extend_from_slice(&trio);
    let heuristic = run(&diet);
    // gamma's `use crate::scip::alpha::helper;` now binds through the rust
    // module plane, agreeing with scip below: no ambiguity left for a real
    // index to fix here (the plane's own drops-channel + fixture ambiguity
    // is covered by 57_rust_module_plane.rs).
    assert!(
        heuristic
            .lines()
            .any(|line| line.contains("\"record\":\"resolved_edge\"")
                && line.contains("\"kind\":\"import_resolve\"")
                && line.contains("\"caller_name\":\"run\"")
                && line.contains("\"callee_name\":\"helper\"")
                && line.contains("alpha.rs")),
        "the module plane binds gamma's call through its use: {heuristic}"
    );
    assert!(
        !heuristic.contains("beta.rs\",\"callee_name\":\"helper\""),
        "nothing should still bind to beta's helper: {heuristic}"
    );

    let real = scip_family(RUST_ROOT, &cache, &[]);
    assert!(
        real.lines().any(|line| line
            == r#"{"record":"scip_fn_edge","caller":"rust-analyzer cargo fixtures 0.0.0 scip/gamma/run().","callee":"rust-analyzer cargo fixtures 0.0.0 scip/alpha/helper()."}"#),
        "rust-analyzer must bind gamma's call to alpha: {real}"
    );
    assert!(
        !real.contains("scip/beta/helper()\",")
            && !real.contains("callee\":\"rust-analyzer cargo fixtures 0.0.0 scip/beta/helper()."),
        "nothing references beta's helper: {real}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// THE GOLDENS
// ════════════════════════════════════════════════════════════════════════════

/// The whole `--family scip` stream over the relationship fixture, pinned.
///
/// This corpus is one file and produces every relation an interface hierarchy
/// can: `scip_impl` is the pair from scip.proto's own worked example (`Dog#`
/// implements `Animal#`, `Dog#sound()` implements `Animal#sound()`), which
/// occurrences alone never carry.
#[test]
fn the_scip_family_stream_is_the_v5_relation_vocabulary() {
    let cache = scratch("golden-scip");
    assert_eq!(scip_family(SCIP_REL_ROOT, &cache, &[]), SCIP_REL_GOLDEN);
}

/// The whole `--family diet_scip` stream over four ts files, pinned. Every row
/// is a resolve-pass record (`resolved_edge` / `resolved_type_edge`, plus the
/// drops channel's `unresolved` rows): the family is a LABEL on the existing
/// resolve pass, not a new wire.
#[test]
fn the_diet_scip_family_stream_is_the_resolve_pass_output() {
    let stream = run(&[
        "--family",
        "diet_scip",
        "tests/fixtures/ts/sample.ts",
        "tests/fixtures/ts/docs.ts",
        "tests/fixtures/ts/lambdas.ts",
        "tests/fixtures/ts/consts.ts",
    ]);
    assert_eq!(stream, DIET_SCIP_GOLDEN);
    assert!(
        stream
            .lines()
            .all(|line| line.contains("\"resolved_edge\"")
                || line.contains("\"resolved_type_edge\"")
                || line.contains("\"record\":\"unresolved\"")),
        "a diet stream carries only resolve-pass records (edges + drops): {stream}"
    );
}

/// `diet_scip` IS the `--resolve` pass with both arms, byte for byte. Asserting
/// it here does two jobs: it pins what the new name means, and it is the
/// regression guard that `--resolve` (whose own default stays the narrower
/// `call`) was not disturbed by adding the label.
#[test]
fn diet_scip_is_exactly_the_existing_resolve_pass_with_both_arms() {
    let mut labelled: Vec<&str> = vec!["--family", "diet_scip"];
    labelled.extend_from_slice(&TS_TRIO);
    let mut original: Vec<&str> = vec!["--resolve", "--family", "call,type"];
    original.extend_from_slice(&TS_TRIO);
    assert_eq!(run(&labelled), run(&original));

    // The pre-existing spellings are untouched: --resolve alone still defaults
    // to the call arm only, and the phase-1 mask still means the phase-1 mask.
    let mut call_only: Vec<&str> = vec!["--resolve"];
    call_only.extend_from_slice(&TS_TRIO);
    let call_only = run(&call_only);
    assert!(
        !call_only.contains("resolved_type_edge"),
        "--resolve's call-only default must survive: {call_only}"
    );
    let mask = run(&["--family", "cst", "tests/fixtures/ts/scip/alpha.ts"]);
    assert!(
        mask.lines().all(|line| line.contains("\"family\":\"cst\"")),
        "--family cst is still the per-file mask: {mask}"
    );
}

/// rust-analyzer's own version string rides the stream, so the rust corpus is
/// asserted structurally rather than pinned: a toolchain bump must not turn a
/// green suite red. What IS pinned is that every v5 relation the rust plane can
/// produce shows up, including the two rust-analyzer alone reaches:
/// `scip_callee_type` (the `impl#[T]` receiver parse) and `scip_local` (the
/// per-document `local N` join through display_name).
#[test]
fn the_rust_plane_produces_the_relations_only_a_real_index_carries() {
    let cache = scratch("rust-plane");
    let stream = scip_family(RUST_ROOT, &cache, &[]);

    for kind in [
        "scip_def",
        "scip_name",
        "scip_ref",
        "scip_edge",
        "scip_fn_edge",
        "scip_callee_type",
        "scip_local",
    ] {
        assert!(
            !records(&stream, kind).is_empty(),
            "the rust corpus must produce {kind} rows: {stream}"
        );
    }
    // The receiver-type parse: `…/impl#[Engine]mode().` yields `Engine`. A
    // plain descriptor scan cannot get this; it needs the moniker grammar.
    assert!(
        stream.lines().any(|line| line
            == r#"{"record":"scip_callee_type","sym":"rust-analyzer cargo fixtures 0.0.0 sample/impl#[Engine]mode().","type":"Engine"}"#),
        "the impl#[T] receiver parse must land: {stream}"
    );
    // The local join: rust-analyzer emits `local 0`, and the source name lives
    // on the matching SymbolInformation's display_name, keyed PER DOCUMENT.
    assert!(
        stream.lines().any(|line| line
            == r#"{"record":"scip_local","fn":"rust-analyzer cargo fixtures 0.0.0 sample/make_engine().","name":"trimmed"}"#),
        "the per-document local-name join must land: {stream}"
    );
    // One header row, saying an index was built rather than reused.
    let header = records(&stream, "scip_index");
    assert_eq!(header.len(), 1, "exactly one index header: {header:?}");
    assert!(
        header[0].contains("\"reused\":false")
            && header[0].contains("\"tool_name\":\"rust-analyzer\""),
        "the header must name the tool that answered: {}",
        header[0]
    );
}

// ════════════════════════════════════════════════════════════════════════════
// ENSURE-INDEX: REUSE, AND THE THREE NAMED SKIPS
// ════════════════════════════════════════════════════════════════════════════

/// AN EXISTING INDEX WINS UNTOUCHED (v5's first move). The second run over the
/// same cache reuses, and the assertion is not just the `reused` flag: the rows
/// must be IDENTICAL, because a reuse that produced different facts would mean
/// the cache and the indexer disagree, which is worse than not caching.
#[test]
fn an_existing_index_is_reused_and_yields_identical_rows() {
    let cache = scratch("reuse");
    let built = scip_family(SCIP_REL_ROOT, &cache, &[]);
    let reused = scip_family(SCIP_REL_ROOT, &cache, &[]);

    assert!(built.contains("\"reused\":false"), "first run builds");
    assert!(reused.contains("\"reused\":true"), "second run reuses");
    assert_eq!(
        built.replace("\"reused\":false", "\"reused\":true"),
        reused,
        "the reused index must yield the same facts as the built one"
    );
}

/// NO TOOLCHAIN IS A NAMED SKIP, NOT A FAILURE AND NOT SILENCE.
///
/// v5's law is that a missing indexer skips the root and never fails the tick,
/// so the exit code is 0. But an empty stream reads as "this project has no
/// symbols", which is a worse lie than a failure, so the reason rides the
/// stream as a row carrying the language, the binary and the install command.
#[test]
fn a_root_with_no_installed_indexer_emits_a_named_skip_and_exits_zero() {
    let cache = scratch("no-toolchain");
    let empty_path = scratch("no-toolchain-path");
    let cache_arg = cache.to_string_lossy().to_string();

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        // An empty PATH: no rust-analyzer, no scip-typescript, no npx, no go.
        .env("PATH", &empty_path)
        .args(["--family", "scip", "--scip-cache", &cache_arg, RUST_ROOT])
        .output()
        .expect("extract binary runs");

    assert!(
        output.status.success(),
        "a missing toolchain skips a root; it never kills the caller (exit {})",
        output.status
    );
    let stream = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        stream,
        "{\"record\":\"scip_skip\",\"lang\":\"rust\",\"bin\":\"rust-analyzer\",\
         \"reason\":\"not_installed\",\"detail\":\"not on PATH; install: rustup \
         component add rust-analyzer\"}\n",
        "the skip must name the language, the binary and how to fix it"
    );
}

/// A directory that is not a project root at all. The row says so, and says
/// what would have worked, rather than leaving the caller to guess whether the
/// project is empty or the path is wrong.
#[test]
fn a_root_with_no_marker_file_says_so_rather_than_streaming_nothing() {
    let root = scratch("no-markers");
    let cache = scratch("no-markers-cache");
    let stream = scip_family(&root.to_string_lossy(), &cache, &[]);

    let skips = records(&stream, "scip_skip");
    assert_eq!(skips.len(), 1, "one skip row: {stream}");
    assert!(
        skips[0].contains("\"reason\":\"no_markers\"")
            && skips[0].contains("Cargo.toml")
            && skips[0].contains("go.mod")
            && skips[0].contains("tsconfig.json"),
        "the row must name every marker the roster looks for: {}",
        skips[0]
    );
}

/// THE BUDGET, AND WHY THE KILL TARGETS THE PROCESS GROUP.
///
/// The planted indexer forks a grandchild that writes a heartbeat file forever,
/// then sleeps far past the budget. Three things are asserted and the third is
/// the one that matters:
///   1. the run returns near the deadline rather than at the sleep's end;
///   2. the skip row names `timed_out` and the budget;
///   3. THE GRANDCHILD IS DEAD.
///
/// Without (3) this test passes on a bound that kills only the direct child and
/// leaves the real work running, reparented and invisible. That is exactly the
/// shape the timeout-gun law exists to stop, and it is exactly what these
/// indexers do: rust-analyzer forks cargo metadata, scip-typescript forks tsc.
#[test]
#[cfg(unix)]
fn an_indexer_past_its_budget_is_killed_with_its_whole_process_group() {
    use std::time::Instant;

    let bin_dir = scratch("budget-bin");
    let root = scratch("budget-root");
    let cache = scratch("budget-cache");
    let beat = root.join("heartbeat");
    let pidfile = root.join("grandchild.pid");

    // A root that detects as rust, so the planted binary is the one probed.
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"slow\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn a() {}\n").unwrap();

    let planted = bin_dir.join("rust-analyzer");
    std::fs::write(
        &planted,
        format!(
            "#!/bin/sh\n\
             ( while true; do date +%s > '{beat}'; sleep 0.2; done ) &\n\
             echo $! > '{pid}'\n\
             sleep 120\n",
            beat = beat.display(),
            pid = pidfile.display(),
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // The planted dir goes FIRST so its rust-analyzer wins the probe; the
    // system dirs follow because the planted script itself needs `sleep` and
    // `date`, and no real indexer lives in either of them.
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env("PATH", format!("{}:/bin:/usr/bin", bin_dir.display()))
        .args([
            "--family",
            "scip",
            "--scip-timeout",
            "2",
            "--scip-cache",
            &cache.to_string_lossy(),
            &root.to_string_lossy(),
        ])
        .output()
        .expect("extract binary runs");
    let wall = started.elapsed();

    assert!(output.status.success(), "a timeout skips, it does not fail");
    assert!(
        wall.as_secs() < 20,
        "the run must return near its 2s budget, not the planted 120s sleep; \
         took {wall:?}"
    );
    let stream = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        stream,
        "{\"record\":\"scip_skip\",\"lang\":\"rust\",\"bin\":\"rust-analyzer\",\
         \"reason\":\"timed_out\",\"detail\":\"exceeded the 2s budget; process \
         group killed\"}\n",
        "the skip must name the budget it exceeded"
    );

    // (3) THE RECEIPT. The grandchild was never touched by name; it died
    // because the signal went to the group.
    let pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("the planted indexer recorded its grandchild")
        .trim()
        .parse()
        .expect("a pid");
    let alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .expect("kill -0 runs")
        .status
        .success();
    if alive {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        panic!(
            "grandchild {pid} outlived the budget: the kill reached the direct \
             child only, so a wedged indexer would leak its real work"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// THE FAMILY VOCABULARY ITSELF
// ════════════════════════════════════════════════════════════════════════════

/// A mode name and a mask name in one `--family` has no honest reading: one
/// selects planes of a single file's extraction, the other runs an indexer over
/// a whole project. Picking one silently would produce a stream the caller did
/// not ask for, so it is an error that names both halves.
#[test]
fn mixing_a_mode_with_the_per_file_mask_is_a_named_error() {
    let output = raw(&["--family", "cst,scip", SCIP_REL_ROOT]);
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("scip") && message.contains("cst"),
        "the error must name the mode and the mask names it cannot join: {message}"
    );

    let both = raw(&["--family", "scip,diet_scip", SCIP_REL_ROOT]);
    assert!(!both.status.success());
    let message = String::from_utf8_lossy(&both.stderr).to_string();
    assert!(
        message.contains("scip") && message.contains("diet_scip"),
        "asking for both answers to the same question must name both: {message}"
    );
}

/// `--family scip` takes ONE root. Several would each need their own indexer
/// run and their own cache, and silently indexing the first would be a lie
/// about the other arguments.
#[test]
fn the_scip_family_takes_exactly_one_root() {
    let output = raw(&["--family", "scip", SCIP_REL_ROOT, TS_ROOT]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("one ROOT"),
        "the error must say what it wanted"
    );
}

/// A mask name outside cst/type/call/df is a named stop, not a silent empty
/// stream. The mode names are consumed by `family_mode` before `parse_mask`
/// runs, so anything unknown here is a typo.
#[test]
fn an_unknown_mask_family_is_a_named_error() {
    let output = raw(&["--family", "nonsense", "tests/fixtures/ts/scip/alpha.ts"]);
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("nonsense") && message.contains("mask family"),
        "the error must name the unknown family: {message}"
    );
}

/// The honest-label sentence has to be reachable from the binary itself, not
/// only from the source. A caller who reads `--help` or `--schema` must learn
/// what `diet` means before they trust a diet row.
#[test]
fn the_binary_states_what_diet_means() {
    const SENTENCE: &str = "DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA";
    let schema = run(&["--schema"]);
    assert!(schema.contains(SENTENCE), "missing from --schema");

    // --help states the same fact in its own words (help.rs LONG_ABOUT).
    const HELP_SENTENCE: &str = "\"diet\" names the technique";
    let help = String::from_utf8_lossy(&raw(&["--help"]).stdout).to_string();
    assert!(help.contains(HELP_SENTENCE), "missing from --help: {help}");

    // And the record vocabulary the scip family emits is documented, so a
    // consumer can decode the stream without reading this crate.
    for record in [
        "record=scip_def",
        "record=scip_name",
        "record=scip_ref",
        "record=scip_edge",
        "record=scip_fn_edge",
        "record=scip_callee_type",
        "record=scip_local",
        "record=scip_impl",
        "record=scip_skip",
        "record=scip_index",
    ] {
        assert!(schema.contains(record), "--schema is missing {record}");
    }
}

/// v5's `scip_occurrence` and `scip_binding` are NOT in the family, and the
/// reason is a wire collision rather than a gap: `scip_occurrence` is already a
/// record tag on this wire, carrying byte spans under `--scip-facts`. This test
/// pins that the two streams do not both claim the tag, which is what would
/// make the omission a bug instead of a decision.
#[test]
fn the_scip_family_never_reuses_the_passthrough_occurrence_tag() {
    let cache = scratch("no-tag-collision");
    let family = scip_family(SCIP_REL_ROOT, &cache, &[]);
    assert!(
        records(&family, "scip_occurrence").is_empty()
            && records(&family, "scip_binding").is_empty(),
        "neither tag may appear in the family stream: {family}"
    );

    // The passthrough row that owns the tag is still reachable, still carrying
    // byte spans, and is what a consumer joins to rebuild either v5 row.
    let passthrough = run(&[
        "--scip-facts",
        "--scip-record",
        "scip_occurrence",
        "--project-root",
        SCIP_REL_ROOT,
        "--scip-index",
        &cache.join("index.scip").to_string_lossy(),
        // Under --scip-facts the positional only picks the indexer for a
        // --scip-build; the facts cover the whole index either way.
        "tests/fixtures/scip_rel/animal.ts",
    ]);
    let occurrences = records(&passthrough, "scip_occurrence");
    assert!(!occurrences.is_empty(), "the tag's owner still streams");
    assert!(
        occurrences[0].contains("\"start\":") && occurrences[0].contains("\"definition\":"),
        "and still carries the spans and role bits: {}",
        occurrences[0]
    );
}

// ════════════════════════════════════════════════════════════════════════════
// THE ROSTER IS v5's SIX
// ════════════════════════════════════════════════════════════════════════════

/// Every row of v5's INDEXERS table (`src/scip_setup.rs:51-99`) reaches a
/// `ScipSource`. The lang strings are v5's, so a rename here is a rename there.
#[test]
fn the_roster_carries_v5s_six_languages() {
    let langs: Vec<&str> = sprefa_extract::INDEXERS.iter().map(|ix| ix.lang).collect();
    assert_eq!(
        langs,
        vec!["rust", "typescript", "python", "go", "kotlin/java", "cpp"],
        "v5's six rows, in v5's order"
    );
    let bins: Vec<&str> = sprefa_extract::INDEXERS.iter().map(|ix| ix.bin).collect();
    assert_eq!(
        bins,
        vec![
            "rust-analyzer",
            "scip-typescript",
            "scip-python",
            "scip-go",
            "scip-java",
            "scip-clang"
        ],
        "v5's binaries, verbatim"
    );
    for ix in sprefa_extract::INDEXERS {
        assert_eq!(
            ix.source.indexer(),
            ix.bin,
            "{}: the roster row and its ScipSource must name the same binary",
            ix.lang
        );
        assert!(!ix.markers.is_empty(), "{}: no marker files", ix.lang);
        assert!(!ix.install.is_empty(), "{}: no install hint", ix.lang);
    }
}

/// A marker for a language whose indexer is absent is a NAMED SKIP with the
/// install hint, exit 0. The three languages this arc added are the ones with
/// no toolchain on this machine, so the empty-PATH plant is not needed to make
/// them miss; it is planted anyway so the test says the same thing everywhere.
#[test]
fn the_three_added_languages_detect_and_skip_by_name() {
    for (marker, lang, bin) in [
        ("pyproject.toml", "python", "scip-python"),
        ("build.gradle.kts", "kotlin/java", "scip-java"),
        ("compile_commands.json", "cpp", "scip-clang"),
    ] {
        let root = scratch(&format!("added-{lang}").replace('/', "-"));
        let cache = scratch(&format!("added-{lang}-cache").replace('/', "-"));
        let empty_path = scratch(&format!("added-{lang}-path").replace('/', "-"));
        std::fs::write(root.join(marker), "").expect("plant the marker");

        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .env("PATH", &empty_path)
            .args([
                "--family",
                "scip",
                "--scip-cache",
                &cache.to_string_lossy(),
                &root.to_string_lossy(),
            ])
            .output()
            .expect("extract binary runs");

        assert!(
            output.status.success(),
            "{lang}: a missing toolchain skips the root, exit was {}",
            output.status
        );
        let stream = String::from_utf8_lossy(&output.stdout).to_string();
        let skips = records(&stream, "scip_skip");
        assert_eq!(skips.len(), 1, "{lang}: one skip row, got: {stream}");
        assert!(
            skips[0].contains(&format!("\"lang\":\"{lang}\""))
                && skips[0].contains(&format!("\"bin\":\"{bin}\""))
                && skips[0].contains("\"reason\":\"not_installed\""),
            "{lang}: the row must name the language, the binary and the reason: {}",
            skips[0]
        );
    }
}
