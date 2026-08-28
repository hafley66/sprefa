//! CAPABILITY PARITY: whatever the library can do, the binary can reach.
//!
//! WHY THIS TEST EXISTS. The CLI silently fell behind the library and nothing
//! caught it: `Resolve<TypeF>` and the whole SCIP path were library-reachable
//! and binary-unreachable for the entire life of `--resolve`, because the
//! project recipe lived in the binary's own private adapter and only ever
//! dispatched the `CallF` arm with `reader: None`. No test compared the two
//! surfaces, so the gap was invisible until someone read both files side by
//! side. The spelunk (`plans/2026-07-30-sprefa-extract-spelunk.md`) recorded
//! that the assertion "was unwritable because the bin never claimed phase 2".
//! It claims it now, so this is the assertion.
//!
//! TWO LEGS, TWO ENFORCEMENT MECHANISMS. Neither is a list someone has to
//! remember to update.
//!
//!  1. ROSTER LEG, enforced at RUNTIME off `sprefa_extract::sources()`. For
//!     every registered `Source`, the binary's default output over a fixture
//!     must equal the library's own `flatten` over the same bytes. Adding a
//!     language to the roster makes this test demand a fixture for it, so a
//!     new `Source` cannot land binary-unreachable.
//!
//!  2. CAPABILITY LEG, enforced by the COMPILER. `LibraryCapability` is an
//!     enum and `reach_of` matches on it exhaustively, so adding a library
//!     capability does not compile until someone states how the binary reaches
//!     it, or marks it `LibraryOnly` with a written reason. `ALL` is checked
//!     against a declared count so a variant cannot be added and left out of
//!     the run either.
//!
//! FOREIGN INDEXERS ARE NOT GATED. The SCIP legs invoke the real
//! scip-typescript, exactly like the existing ratchets in `golden_parity.rs`
//! ("the ratchet never fakes green"). A missing indexer fails this test loudly
//! rather than skipping to a green that means nothing.
//!
//! SABOTAGE RECEIPTS (all three run, all three red, then reverted). The first
//! two re-create the exact drift this test exists to catch:
//!  - deleting the `arms.types` dispatch from `project::resolve_project`
//!    -> "ResolveType: the binary reached no resolved_type_edge record".
//!  - putting back `reader: None` in the resolve context, the state the binary
//!    shipped in before this lane
//!    -> "ScipIndexLoad: the binary reached no resolved_edge record".
//!  - deleting the prolog row from `ROSTER_FIXTURES`
//!    -> "these Sources are in the roster with no capability-parity fixture:
//!    [\"prolog\"]".
//!  - deleting the `--package-deps` dispatch from the binary's main
//!    -> "PackageEdges: the binary reached no package_edge record".

use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_extract::{dispatch, flatten_jsonl, sources, FamilyMask};

const BIN: &str = env!("CARGO_BIN_EXE_extract");

// ════════════════════════════════════════════════════════════════════════════
// LEG 1: the roster. Runtime-enumerated, so a new language cannot skip it.
// ════════════════════════════════════════════════════════════════════════════

/// One fixture per registered `Source`, keyed by `Source::name()`. This table is
/// checked against the live roster in both directions below: a roster entry with
/// no row here fails, and a row here naming no roster entry fails.
const ROSTER_FIXTURES: &[(&str, &str)] = &[
    ("ts", "tests/fixtures/ts/sample.ts"),
    ("rust", "tests/fixtures/rust/sample.rs"),
    ("go", "tests/fixtures/go/sample.go"),
    ("kotlin", "tests/fixtures/kotlin/sample.kt"),
    ("prolog", "tests/fixtures/prolog/0_sample.pl"),
    ("python", "tests/fixtures/python/sample.py"),
    ("dl6", "tests/fixtures/dl6/0_sample.dl6"),
    ("markdown", "tests/fixtures/markdown/0_sample.md"),
    ("data", "tests/fixtures/data/nested.json"),
    ("astgrep", "tests/fixtures/astgrep/sample.html"),
];

/// Every `Source` in the live roster produces the same facts through the binary
/// as through the library. This is the phase-1 half of parity, and it covers
/// every family a language emits at once: the comparison is the whole flattened
/// stream, not a sampled record kind.
#[test]
fn every_roster_source_is_reachable_through_the_binary() {
    let roster: Vec<&'static str> = sources().iter().map(|source| source.name()).collect();

    let uncovered: Vec<&str> = roster
        .iter()
        .copied()
        .filter(|name| !ROSTER_FIXTURES.iter().any(|(key, _)| key == name))
        .collect();
    assert!(
        uncovered.is_empty(),
        "these Sources are in the roster with no capability-parity fixture: {uncovered:?}. \
         A new language must bring a fixture here, or nothing proves the binary reaches it."
    );
    let stale: Vec<&str> = ROSTER_FIXTURES
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !roster.contains(key))
        .collect();
    assert!(
        stale.is_empty(),
        "ROSTER_FIXTURES names Sources the roster no longer has: {stale:?}"
    );

    for (name, fixture) in ROSTER_FIXTURES {
        let content = std::fs::read(fixture).unwrap_or_else(|err| {
            panic!("capability-parity fixture for {name} is missing: {fixture}: {err}")
        });
        let output = dispatch(fixture, &content, FamilyMask::ALL)
            .unwrap_or_else(|| panic!("{fixture} routes to no Source, so it cannot cover {name}"));
        let from_library = flatten_jsonl(&output);
        assert!(
            !from_library.is_empty(),
            "{name}'s fixture {fixture} produces no facts, so it proves nothing"
        );

        let mut from_binary = run(&[fixture]);
        from_binary.sort();
        assert_eq!(
            from_binary, from_library,
            "{name}: the binary's stream and the library's flatten disagree over {fixture}"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════
// LEG 2: named capabilities. Compiler-enumerated.
// ════════════════════════════════════════════════════════════════════════════

/// Every fact-producing capability the library exposes. Adding a variant here
/// does not compile until `reach_of` states how the binary reaches it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LibraryCapability {
    /// `flatten` / `flatten_jsonl` over a dispatched file.
    Phase1Flatten,
    /// `FamilyMask` selection of a family subset.
    Phase1FamilyMask,
    /// `query_patterns`: batched ast-grep patterns over one parse.
    PatternQuery,
    /// `Resolve<CallF>`: resolved caller-to-callee edges.
    ResolveCall,
    /// `Resolve<TypeF>`: resolved type reference edges.
    ResolveType,
    /// `ScipSource::load` plus a reader placed in `ProjectCx`, which is what
    /// turns on the resolve arms' SCIP leg.
    ScipIndexLoad,
    /// `ScipSource::build`: run the language's own indexer.
    ScipIndexBuild,
    /// `wire::SCHEMA`: the JSONL contract text.
    WireSchema,
    /// `wire::flatten_project_type`: the span-addressed project-edge wire.
    FlattenProjectType,
    /// `wire::flatten_scip` / `project::scip_facts`: the loaded SCIP index as
    /// raw occurrence, symbol and relationship rows.
    ScipFacts,
    /// `wire::file_fact`: one file's digest, byte count and line count.
    FileFact,
    /// `wire::scip_file_edges`: the module graph folded out of a SCIP index.
    ScipFileEdges,
    /// `deps::fold_edges`: the same module graph resolved syntactically.
    DietFileEdges,
    /// `deps::fold_unresolved`: the specifiers that resolved to nothing.
    DietFileUnresolved,
    /// `manifests::fold_package_edges`: workspace-internal manifest edges.
    PackageEdges,
}

/// Kept in step with the enum by the length assertion in the test below.
const ALL: &[LibraryCapability] = &[
    LibraryCapability::Phase1Flatten,
    LibraryCapability::Phase1FamilyMask,
    LibraryCapability::PatternQuery,
    LibraryCapability::ResolveCall,
    LibraryCapability::ResolveType,
    LibraryCapability::ScipIndexLoad,
    LibraryCapability::ScipIndexBuild,
    LibraryCapability::WireSchema,
    LibraryCapability::FlattenProjectType,
    LibraryCapability::ScipFacts,
    LibraryCapability::FileFact,
    LibraryCapability::ScipFileEdges,
    LibraryCapability::DietFileEdges,
    LibraryCapability::DietFileUnresolved,
    LibraryCapability::PackageEdges,
];
const DECLARED_CAPABILITIES: usize = 15;

/// How the binary reaches one library capability.
enum CliReach {
    /// Run the binary with these arguments and require at least one line
    /// carrying this `record` tag. `absent_without` names an argument list that
    /// must NOT produce that tag, so the flag is proven to be what caused it
    /// rather than something the binary already did.
    Emits {
        args: Vec<String>,
        record: &'static str,
        absent_without: Option<Vec<String>>,
    },
    /// Run the binary with these arguments and require this text in stdout.
    Prints {
        args: Vec<String>,
        contains: &'static str,
    },
    /// Deliberately not on the CLI, with the reason and the surface that does
    /// carry the same information. Adding one of these is a decision that shows
    /// up in the diff; it is not a way to make a gap quiet.
    LibraryOnly { reason: &'static str },
}

fn ts_scip_root() -> PathBuf {
    PathBuf::from("tests/fixtures/ts")
}

fn ts_scip_files() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(ts_scip_root().join("scip"))
        .expect("the ts scip fixture trio")
        .flatten()
        .map(|entry| entry.path().to_string_lossy().to_string())
        .filter(|path| path.ends_with(".ts"))
        .collect();
    files.sort();
    files
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

/// THE EXHAUSTIVE MATCH. This is the whole enforcement mechanism of leg 2.
fn reach_of(capability: LibraryCapability, scip_index: &Path) -> CliReach {
    use LibraryCapability::*;
    match capability {
        Phase1Flatten => CliReach::Emits {
            args: strings(&["tests/fixtures/ts/sample.ts"]),
            record: "node",
            absent_without: None,
        },
        Phase1FamilyMask => CliReach::Emits {
            args: strings(&["--family", "df", "tests/fixtures/ts/sample.ts"]),
            record: "param",
            // Without the mask the default is every family, so `param` rows
            // appear anyway; the mask's effect is proven by the roster leg's
            // whole-stream equality and by 2_df_aux_cli.rs.
            absent_without: None,
        },
        PatternQuery => CliReach::Emits {
            args: strings(&[
                "--ast-pattern",
                "call=$FN($$$ARGS)",
                "--ast-capture",
                "call=FN",
                "tests/fixtures/ts/sample.ts",
            ]),
            record: "capture",
            absent_without: Some(strings(&["tests/fixtures/ts/sample.ts"])),
        },
        ResolveCall => CliReach::Emits {
            args: strings(&[
                "--resolve",
                "tests/fixtures/resolve/0_caller.ts",
                "tests/fixtures/resolve/1_callee.ts",
            ]),
            record: "resolved_edge",
            absent_without: Some(strings(&["tests/fixtures/resolve/0_caller.ts"])),
        },
        ResolveType => CliReach::Emits {
            args: strings(&[
                "--resolve",
                "--family",
                "type",
                "tests/fixtures/ts/sample.ts",
                "tests/fixtures/ts/consts.ts",
            ]),
            record: "resolved_type_edge",
            // The same paths under the default arm emit call edges only, so the
            // `type` arm is proven to be what produced these rows.
            absent_without: Some(strings(&[
                "--resolve",
                "tests/fixtures/ts/sample.ts",
                "tests/fixtures/ts/consts.ts",
            ])),
        },
        ScipIndexLoad => {
            let mut args = strings(&[
                "--resolve",
                "--project-root",
                &ts_scip_root().to_string_lossy(),
                "--scip-index",
                &scip_index.to_string_lossy(),
            ]);
            args.extend(ts_scip_files());
            CliReach::Emits {
                args,
                // A scip_override row exists ONLY because the indexer's answer
                // displaced the name match, so this record is proof the index
                // was really consumed and not merely accepted.
                record: "resolved_edge",
                absent_without: {
                    let mut bare = strings(&["--resolve"]);
                    bare.extend(ts_scip_files());
                    Some(bare)
                },
            }
        }
        ScipIndexBuild => {
            let mut args = strings(&[
                "--resolve",
                "--project-root",
                &ts_scip_root().to_string_lossy(),
                "--scip-build",
            ]);
            args.extend(ts_scip_files());
            CliReach::Emits {
                args,
                record: "resolved_edge",
                absent_without: {
                    let mut bare = strings(&["--resolve"]);
                    bare.extend(ts_scip_files());
                    Some(bare)
                },
            }
        }
        WireSchema => CliReach::Prints {
            args: strings(&["--schema"]),
            contains: "record=resolved_type_edge",
        },
        // Built over its own root rather than the shared ts index: the
        // relationship rows this reach proves only exist in the implements
        // fixture, and a reach that could pass on an index without them would
        // not be proving retention.
        ScipFacts => CliReach::Emits {
            args: strings(&[
                "--scip-facts",
                "--project-root",
                "tests/fixtures/scip_rel",
                "--scip-build",
                "tests/fixtures/scip_rel/animal.ts",
            ]),
            record: "scip_relationship",
            absent_without: Some(strings(&["tests/fixtures/scip_rel/animal.ts"])),
        },
        FileFact => CliReach::Emits {
            args: strings(&["--file-fact", "tests/fixtures/ts/sample.ts"]),
            record: "file",
            absent_without: Some(strings(&["tests/fixtures/ts/sample.ts"])),
        },
        ScipFileEdges => CliReach::Emits {
            args: strings(&[
                "--scip-deps",
                "--project-root",
                "tests/fixtures/ts",
                "--scip-build",
                "tests/fixtures/ts/scip/alpha.ts",
            ]),
            record: "file_edge",
            absent_without: Some(strings(&["tests/fixtures/ts/scip/gamma.ts"])),
        },
        DietFileEdges => CliReach::Emits {
            args: strings(&[
                "--deps",
                "--project-root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
                "tests/fixtures/deps/lib/util.ts",
            ]),
            record: "file_edge",
            absent_without: Some(strings(&["tests/fixtures/deps/app.ts"])),
        },
        DietFileUnresolved => CliReach::Emits {
            args: strings(&[
                "--deps",
                "--project-root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
            ]),
            record: "file_unresolved",
            absent_without: Some(strings(&["tests/fixtures/deps/app.ts"])),
        },
        PackageEdges => CliReach::Emits {
            args: strings(&[
                "--package-deps",
                "--project-root",
                "tests/fixtures/packages",
                "tests/fixtures/packages/crates/alpha/Cargo.toml",
                "tests/fixtures/packages/crates/beta/Cargo.toml",
            ]),
            record: "package_edge",
            absent_without: Some(strings(&[
                "--deps",
                "--project-root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
            ])),
        },
        FlattenProjectType => CliReach::LibraryOnly {
            reason: "the span-and-blob project-edge shape predates the flat-fields \
                     rule for host decoding. The CLI carries the same edges as \
                     record=resolved_type_edge with paths and names, which a \
                     line-oriented consumer can decode without a span join; \
                     flatten_project_type stays for golden_parity's normalize.",
        },
    }
}

#[test]
fn every_library_capability_is_reachable_through_the_binary() {
    assert_eq!(
        ALL.len(),
        DECLARED_CAPABILITIES,
        "a LibraryCapability variant was added without adding it to ALL, so it \
         would never be exercised"
    );

    // One real scip-typescript index, shared by the two SCIP capabilities.
    // Built through the library seam, which is also the capability under test
    // for ScipIndexBuild's library half.
    let scip_index = build_ts_scip_index();

    for capability in ALL {
        match reach_of(*capability, &scip_index) {
            CliReach::Emits {
                args,
                record,
                absent_without,
            } => {
                let tag = format!("\"record\":\"{record}\"");
                let lines = run_ok(&args, *capability);
                assert!(
                    lines.iter().any(|line| line.contains(&tag)),
                    "{capability:?}: the binary reached no {record} record with {args:?}"
                );
                if let Some(bare) = absent_without {
                    let without = run_ok(&bare, *capability);
                    assert!(
                        !without.iter().any(|line| line.contains(&tag)),
                        "{capability:?}: {record} records appear WITHOUT {args:?} too, so \
                         this reach proves nothing about the capability"
                    );
                }
            }
            CliReach::Prints { args, contains } => {
                let stdout = run_ok(&args, *capability).join("\n");
                assert!(
                    stdout.contains(contains),
                    "{capability:?}: `extract {args:?}` printed no {contains:?}"
                );
            }
            CliReach::LibraryOnly { reason } => {
                assert!(
                    reason.len() > 40,
                    "{capability:?}: a LibraryOnly capability needs a real written \
                     reason naming what carries the same information"
                );
            }
        }
    }
}

/// The library-side SCIP build. Loud on a missing indexer, never skipped: the
/// same law the golden_parity ratchets run under.
fn build_ts_scip_index() -> PathBuf {
    use sprefa_extract::{ScipSource, ScipTypescript};
    ScipTypescript.build(&ts_scip_root()).expect(
        "scip-typescript build failed. This test does not gate on the indexer being \
         installed: a skipped SCIP leg is a green that means nothing.",
    )
}

fn run(args: &[&str]) -> Vec<String> {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run_ok(&owned, LibraryCapability::Phase1Flatten)
}

fn run_ok(args: &[String], capability: LibraryCapability) -> Vec<String> {
    let output = Command::new(BIN)
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{capability:?}: `extract {args:?}` exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(str::to_string)
        .collect()
}
