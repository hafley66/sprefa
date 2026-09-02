//! Resolver invariants under corpus mutation: copy a fixture set into a temp
//! dir, apply one text mutation, re-resolve, and diff the resolved edges by
//! `resolution_origin` (plans/extract-eval-2026-08-31/PLAN.md sec 5, arc D).
//!
//! Invariants under test:
//! 1. duplicate-def: a def named N copied into a NEW corpus file must flip
//!    every `corpus_unique` edge to N ABSENT. Present with any dst is a guess.
//! 2. relocation: a def moved to another file keeps its edges; only dst_path
//!    moves. Identity is (owner/caller name, callee name), never byte spans:
//!    text edits above a site legally shift offsets (asserted by the rust
//!    move, whose use-line rewrite moves every later owner span).
//! 3. shadow: a same-named parameter above a call site must drop the edge or
//!    re-point it to the param binding, never keep the module-level def.
//! 4. origin conservation: for every mutation, `same_file`/`checker` edges
//!    that do not name the mutated def are byte-identical rows.
//!
//! Expected-red rows, kept #[ignore] so the defects stay named in-tree:
//! - F1 shadow_python: a param named imported_fn does not shadow the
//!   module-level import; the call inside the enclosing def keeps a
//!   corpus_unique edge to helper.py's def.
//! - F5 duplicate_def_python_same_file: a second local_fn in another file
//!   leaves main.py's corpus_unique edge alive; the cross-file duplicate
//!   (duplicate_def_python_cross_file, green) does flip to absent, so the
//!   drop-on-ambiguity behavior is inconsistent between the two shapes.
//!
//! rust has no duplicate-def row: no rust leg answers `corpus_unique`
//! (survey: same_file/module_plane/receiver/self_type only), so the
//! invariant's premise never arises. python and ts mint no `same_file` call
//! edges, so their conservation rows hold over an empty set.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest};

const SAME_FILE: &str = "same_file";
const CHECKER: &str = "checker";
const CORPUS_UNIQUE: &str = "corpus_unique";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CallEdge {
    caller_path: String,
    caller_name: Option<String>,
    callee_path: String,
    callee_name: Option<String>,
    kind: String,
    origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TypeEdge {
    owner_path: String,
    owner_name: Option<String>,
    target_path: String,
    target_name: Option<String>,
    kind: String,
    origin: String,
}

/// The def an edge points at, the name a mutation is allowed to disturb.
trait SubjectRow {
    fn origin(&self) -> &str;
    fn subject(&self) -> Option<&str>;
}

impl SubjectRow for CallEdge {
    fn origin(&self) -> &str {
        &self.origin
    }
    fn subject(&self) -> Option<&str> {
        self.callee_name.as_deref()
    }
}

impl SubjectRow for TypeEdge {
    fn origin(&self) -> &str {
        &self.origin
    }
    fn subject(&self) -> Option<&str> {
        self.target_name.as_deref()
    }
}

#[derive(Default)]
struct Rows {
    calls: BTreeSet<CallEdge>,
    types: BTreeSet<TypeEdge>,
}

impl Rows {
    fn calls_to(&self, name: &str) -> Vec<&CallEdge> {
        self.calls
            .iter()
            .filter(|edge| edge.callee_name.as_deref() == Some(name))
            .collect()
    }

    fn type_edges_to(&self, name: &str) -> Vec<&TypeEdge> {
        self.types
            .iter()
            .filter(|edge| edge.target_name.as_deref() == Some(name))
            .collect()
    }
}

/// Corpus-relative name of a path the resolve echoed back, so base and after
/// rows compare equal regardless of which temp dir staged them.
fn rel_name(dir: &Path, path: String) -> String {
    Path::new(&path)
        .strip_prefix(dir)
        .map(|rel| rel.to_string_lossy().into_owned())
        .unwrap_or(path)
}

fn resolve(dir: &Path, rels: &[&str]) -> Rows {
    let paths: Vec<PathBuf> = rels.iter().map(|rel| dir.join(rel)).collect();
    let facts = resolve_project(&ResolveRequest {
        paths: &paths,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: Default::default(),
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        witness: false,
    })
    .expect("the mutated corpus resolves");
    let mut rows = Rows::default();
    for fact in facts {
        match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                kind,
                resolution_origin,
                ..
            } => {
                rows.calls.insert(CallEdge {
                    caller_path: rel_name(dir, caller_path),
                    caller_name,
                    callee_path: rel_name(dir, callee_path),
                    callee_name,
                    kind,
                    origin: resolution_origin,
                });
            }
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                target_path,
                target_name,
                kind,
                resolution_origin,
                ..
            } => {
                rows.types.insert(TypeEdge {
                    owner_path: rel_name(dir, owner_path),
                    owner_name,
                    target_path: rel_name(dir, target_path),
                    target_name,
                    kind,
                    origin: resolution_origin,
                });
            }
            _ => {}
        }
    }
    rows
}

/// One fixture file staged into the temp corpus: copy `src` (under the
/// manifest dir) to `dst` (under the corpus dir) without touching the source.
struct Staged {
    src: &'static str,
    dst: &'static str,
}

struct Scenario {
    name: &'static str,
    staged: &'static [Staged],
    base: &'static [&'static str],
    after: &'static [&'static str],
    /// The def the mutation targets; conservation skips its edges.
    subject: &'static str,
    mutate: fn(&Path),
}

fn stage_dir(scenario: &Scenario, suffix: &str, seq: u32) -> PathBuf {
    // The seq keeps concurrent tests (an invariant test and the conservation
    // test re-run the same scenario) from deleting each other's corpus dirs.
    let dir = std::env::temp_dir().join(format!(
        "sprefa-extract-90-{}-{suffix}-{seq}",
        scenario.name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in scenario.staged {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(file.src);
        let dst = dir.join(file.dst);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::copy(&src, &dst).unwrap();
    }
    dir
}

fn run(scenario: &Scenario) -> (Rows, Rows) {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // The base run reads the unmutated copies; the after run re-stages and
    // applies the mutation between the two resolves.
    let base_dir = stage_dir(scenario, "base", seq);
    let base = resolve(&base_dir, scenario.base);
    let after_dir = stage_dir(scenario, "after", seq);
    (scenario.mutate)(&after_dir);
    let after = resolve(&after_dir, scenario.after);
    let _ = std::fs::remove_dir_all(&base_dir);
    let _ = std::fs::remove_dir_all(&after_dir);
    (base, after)
}

fn replace_in(dir: &Path, rel: &str, old: &str, new: &str) {
    let path = dir.join(rel);
    let src = std::fs::read_to_string(&path).unwrap();
    assert!(
        src.contains(old),
        "fixture drifted, snippet no longer present in {rel}: {old:?}"
    );
    std::fs::write(&path, src.replacen(old, new, 1)).unwrap();
}

fn write_rel(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

/// Invariant 1: every `corpus_unique` edge to `subject` is absent after a
/// duplicate def lands in a new file; edges to any other def are untouched.
fn assert_duplicate_def(base: &Rows, after: &Rows, subject: &str) {
    let flipped: Vec<&CallEdge> = base
        .calls
        .iter()
        .filter(|edge| edge.origin == CORPUS_UNIQUE && edge.callee_name.as_deref() == Some(subject))
        .collect();
    assert!(
        !flipped.is_empty(),
        "fixture drifted: no corpus_unique edge to {subject} in the base run"
    );
    let survivors = after.calls_to(subject);
    assert!(
        survivors.is_empty(),
        "corpus_unique edge to {subject} survived a duplicate def; present-with-any-dst is a guess: {survivors:?}"
    );
    let bystanders = |rows: &Rows| -> BTreeSet<CallEdge> {
        rows.calls
            .iter()
            .filter(|edge| edge.callee_name.as_deref() != Some(subject))
            .cloned()
            .collect()
    };
    assert_eq!(
        bystanders(base),
        bystanders(after),
        "a duplicate of {subject} disturbed an edge to another def"
    );
}

/// Invariant 4: `same_file`/`checker` rows not naming `subject` are identical.
fn assert_conserved<R: SubjectRow + Clone + Ord + std::fmt::Debug>(
    base: &BTreeSet<R>,
    after: &BTreeSet<R>,
    subject: &str,
    plane: &str,
) {
    let conserved = |rows: &BTreeSet<R>| -> BTreeSet<R> {
        rows.iter()
            .filter(|row| matches!(row.origin(), SAME_FILE | CHECKER))
            .filter(|row| row.subject() != Some(subject))
            .cloned()
            .collect()
    };
    assert_eq!(
        conserved(base),
        conserved(after),
        "a mutation of {subject} disturbed a same_file/checker {plane} edge it does not name"
    );
}

fn assert_conservation(base: &Rows, after: &Rows, subject: &str) {
    assert_conserved(&base.calls, &after.calls, subject, "call");
    assert_conserved(&base.types, &after.types, subject, "type");
}

// ── python: tests/fixtures/py_findings/module_caller ───────────────────────

const PY: &[Staged] = &[
    Staged {
        src: "tests/fixtures/py_findings/module_caller/main.py",
        dst: "main.py",
    },
    Staged {
        src: "tests/fixtures/py_findings/module_caller/helper.py",
        dst: "helper.py",
    },
];

fn python_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "py-dup-imported-fn",
            staged: PY,
            base: &["main.py", "helper.py"],
            after: &["main.py", "helper.py", "dup.py"],
            subject: "imported_fn",
            mutate: |dir| {
                write_rel(dir, "dup.py", "def imported_fn():\n    return 3\n");
            },
        },
        Scenario {
            name: "py-dup-local-fn",
            staged: PY,
            base: &["main.py", "helper.py"],
            after: &["main.py", "helper.py", "dup_local.py"],
            subject: "local_fn",
            mutate: |dir| {
                write_rel(dir, "dup_local.py", "def local_fn():\n    return 99\n");
            },
        },
        Scenario {
            name: "py-move-imported-fn",
            staged: PY,
            base: &["main.py", "helper.py"],
            after: &["main.py", "helper.py", "moved.py"],
            subject: "imported_fn",
            mutate: |dir| {
                let helper = std::fs::read_to_string(dir.join("helper.py")).unwrap();
                write_rel(dir, "moved.py", &helper);
                write_rel(dir, "helper.py", "# imported_fn moved to moved.py\n");
            },
        },
        Scenario {
            name: "py-shadow-imported-fn",
            staged: PY,
            base: &["main.py", "helper.py"],
            after: &["main.py", "helper.py"],
            subject: "imported_fn",
            mutate: |dir| {
                replace_in(
                    dir,
                    "main.py",
                    "result = local_fn()\nother = imported_fn()\n",
                    "def consume(imported_fn):\n    return imported_fn()\n\n\nresult = local_fn()\nother = consume(print)\n",
                );
            },
        },
    ]
}

/// Invariant 1, the cross-file shape: the def lives in helper.py, the call in
/// main.py, so no same-file preference can mask the ambiguity.
#[test]
fn duplicate_def_python_cross_file() {
    let (base, after) = run(&python_scenarios()[0]);
    assert_duplicate_def(&base, &after, "imported_fn");
}

/// FINDING F5: main.py's own local_fn keeps its edge with origin
/// `corpus_unique` even though local_fn is then declared twice in the corpus.
/// The cross-file twin above does flip to absent, so the leg's
/// drop-on-ambiguity behavior depends on where the surviving def sits.
#[test]
fn duplicate_def_python_same_file() {
    let (base, after) = run(&python_scenarios()[1]);
    assert_duplicate_def(&base, &after, "local_fn");
}

/// FINDING F1: `def consume(imported_fn)` shadows the module-level import for
/// the call inside its body, yet the edge survives as
/// caller=consume -> helper.py imported_fn, origin corpus_unique. The param
/// leg exists (py_findings/args mints origin `param`) but loses precedence to
/// the corpus name match.
#[test]
fn shadow_python() {
    let (base, after) = run(&python_scenarios()[3]);
    assert!(
        !base.calls_to("imported_fn").is_empty(),
        "fixture drifted: base run has no imported_fn edge"
    );
    let survived: Vec<&CallEdge> = after
        .calls
        .iter()
        .filter(|edge| edge.callee_name.as_deref() == Some("imported_fn"))
        .collect();
    // Shadowing rule: the site either drops or re-points to the param binding
    // in main.py (origin `param`). A corpus_unique edge to helper.py's def is
    // the module-level binding leaking past the parameter.
    let leaked: Vec<&&CallEdge> = survived
        .iter()
        .filter(|edge| edge.origin != "param" || !edge.callee_path.ends_with("main.py"))
        .collect();
    assert!(
        leaked.is_empty(),
        "shadowed call site kept the module-level def: {leaked:?}"
    );
}

/// Invariant 2: the def moves helper.py -> moved.py; the call site in main.py
/// is byte-identical text, so the whole row must survive with only callee_path
/// moved.
#[test]
fn relocation_python() {
    let (base, after) = run(&python_scenarios()[2]);
    assert_eq!(
        base.calls.len(),
        after.calls.len(),
        "relocation changed the edge count"
    );
    let base_edge = base
        .calls
        .iter()
        .find(|edge| edge.callee_name.as_deref() == Some("imported_fn"))
        .expect("base run binds imported_fn");
    assert_eq!(base_edge.origin, CORPUS_UNIQUE);
    let after_edge = after
        .calls
        .iter()
        .find(|edge| edge.callee_name.as_deref() == Some("imported_fn"))
        .expect("relocated def lost its edge");
    assert_eq!(
        after_edge.callee_path, "moved.py",
        "edge did not follow the def"
    );
    assert_eq!(after_edge.caller_path, base_edge.caller_path);
    assert_eq!(after_edge.caller_name, base_edge.caller_name);
    assert_eq!(after_edge.kind, base_edge.kind);
    assert_eq!(after_edge.origin, base_edge.origin);
    let bystanders = |rows: &Rows| -> BTreeSet<CallEdge> {
        rows.calls
            .iter()
            .filter(|edge| edge.callee_name.as_deref() != Some("imported_fn"))
            .cloned()
            .collect()
    };
    assert_eq!(bystanders(&base), bystanders(&after));
}

/// Invariant 4 over every python mutation. Python legs mint no `same_file`
/// call edges (survey over py_findings: corpus_unique, param, alias_chain,
/// subscript, return_call, decorator), so the conserved set is empty and this
/// pins that it STAYS empty under mutation.
#[test]
fn origin_conservation_python() {
    for scenario in python_scenarios() {
        let (base, after) = run(&scenario);
        assert_conservation(&base, &after, scenario.subject);
    }
}

// ── go calls: tests/fixtures/go ────────────────────────────────────────────

const GO_CALLS: &[Staged] = &[
    Staged {
        src: "tests/fixtures/go/docs.go",
        dst: "docs.go",
    },
    Staged {
        src: "tests/fixtures/go/sample.go",
        dst: "sample.go",
    },
    Staged {
        src: "tests/fixtures/go/edges.go",
        dst: "edges.go",
    },
    Staged {
        src: "tests/fixtures/go/corpus_1.go",
        dst: "corpus_1.go",
    },
];

const GO_TRIM_BLOCK: &str =
    "// Trim returns its input unchanged.\nfunc Trim(value string) string {\n\treturn value\n}\n\n";

fn go_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "go-dup-trim",
            staged: GO_CALLS,
            base: &["docs.go", "sample.go", "edges.go", "corpus_1.go"],
            after: &["docs.go", "sample.go", "edges.go", "corpus_1.go", "dup.go"],
            subject: "Trim",
            mutate: |dir| {
                write_rel(
                    dir,
                    "dup.go",
                    "package docs\n\nfunc Trim(value string) string {\n\treturn value\n}\n",
                );
            },
        },
        Scenario {
            name: "go-move-trim",
            staged: GO_CALLS,
            base: &["docs.go", "sample.go", "edges.go", "corpus_1.go"],
            after: &[
                "docs.go",
                "sample.go",
                "edges.go",
                "corpus_1.go",
                "moved.go",
            ],
            subject: "Trim",
            mutate: |dir| {
                replace_in(dir, "docs.go", GO_TRIM_BLOCK, "");
                write_rel(dir, "moved.go", &format!("package docs\n\n{GO_TRIM_BLOCK}"));
            },
        },
    ]
}

/// FINDING F2: a second Trim in package docs (dup.go) leaves docs.go's
/// MakeEngine -> Trim edge alive, same dst, origin still corpus_unique. The
/// assertion is scoped to package docs: sample.go's Trim edge lives in
/// package sample, which the duplicate never touches, so that row is a
/// bystander and must survive (it does).
#[test]
fn duplicate_def_go() {
    let (base, after) = run(&go_scenarios()[0]);
    assert!(
        base.calls.iter().any(|edge| edge.caller_path == "docs.go"
            && edge.callee_name.as_deref() == Some("Trim")
            && edge.origin == CORPUS_UNIQUE),
        "fixture drifted: no corpus_unique docs.go -> Trim edge in the base run"
    );
    let survived: Vec<&CallEdge> = after
        .calls
        .iter()
        .filter(|edge| edge.caller_path == "docs.go" && edge.callee_name.as_deref() == Some("Trim"))
        .collect();
    assert!(
        survived.is_empty(),
        "corpus_unique edge to Trim survived a same-package duplicate def: {survived:?}"
    );
    let bystanders = |rows: &Rows| -> BTreeSet<CallEdge> {
        rows.calls
            .iter()
            .filter(|edge| {
                !(edge.caller_path == "docs.go" && edge.callee_name.as_deref() == Some("Trim"))
            })
            .cloned()
            .collect()
    };
    assert_eq!(
        bystanders(&base),
        bystanders(&after),
        "the duplicate disturbed an edge outside package docs"
    );
}

/// FINDING F3: moving Trim to moved.go (same package docs) DROPS docs.go's
/// MakeEngine -> Trim edge. Go package scope spans files, so the call still
/// binds in the language's own terms; the go TYPES leg follows exactly this
/// shape (relocation_go_types, green), the call leg does not.
#[test]
fn relocation_go_call() {
    let (base, after) = run(&go_scenarios()[1]);
    assert_eq!(
        base.calls.len(),
        after.calls.len(),
        "relocation changed the edge count"
    );
    let moved = after
        .calls
        .iter()
        .find(|edge| edge.callee_name.as_deref() == Some("Trim") && edge.caller_path == "docs.go")
        .expect("docs.go's Trim edge did not follow the def to moved.go");
    assert_eq!(moved.callee_path, "moved.go");
}

// ── go types: tests/fixtures/go_type_refs ─────────────────────────────────

const GO_TYPES: &[Staged] = &[
    Staged {
        src: "tests/fixtures/go_type_refs/a.go",
        dst: "a.go",
    },
    Staged {
        src: "tests/fixtures/go_type_refs/b.go",
        dst: "b.go",
    },
    Staged {
        src: "tests/fixtures/go_type_refs/other/c.go",
        dst: "other/c.go",
    },
];

fn go_types_scenario() -> Scenario {
    Scenario {
        name: "go-move-modifier-flags",
        staged: GO_TYPES,
        base: &["a.go", "b.go", "other/c.go"],
        after: &["a.go", "b.go", "other/c.go", "moved.go"],
        subject: "ModifierFlags",
        mutate: |dir| {
            replace_in(dir, "a.go", "type ModifierFlags uint32\n\n", "");
            write_rel(
                dir,
                "moved.go",
                "package typerefs\n\ntype ModifierFlags uint32\n",
            );
        },
    }
}

/// Invariant 2 on the types plane: ModifierFlags moves a.go -> moved.go
/// inside package typerefs; b.go is unedited, so its owner spans must hold.
#[test]
fn relocation_go_types() {
    let (base, after) = run(&go_types_scenario());
    assert_eq!(
        base.types.len(),
        after.types.len(),
        "relocation changed the type edge count"
    );
    let moved: Vec<&TypeEdge> = after.type_edges_to("ModifierFlags");
    assert_eq!(
        moved.len(),
        base.type_edges_to("ModifierFlags").len(),
        "edges to ModifierFlags lost by the move"
    );
    assert!(
        moved
            .iter()
            .all(|edge| edge.target_path == "moved.go" && edge.owner_path == "b.go"),
        "edge did not follow the def: {moved:?}"
    );
    let bystanders = |rows: &Rows| -> BTreeSet<TypeEdge> {
        rows.types
            .iter()
            .filter(|edge| edge.target_name.as_deref() != Some("ModifierFlags"))
            .cloned()
            .collect()
    };
    assert_eq!(bystanders(&base), bystanders(&after));
}

/// Invariant 4 over every go mutation: the go_type_refs same_file type edges
/// (Snapshot -> a.go, ModifierList -> b.go) are bystanders of the
/// ModifierFlags move and must be byte-identical rows.
#[test]
fn origin_conservation_go() {
    for scenario in go_scenarios().into_iter().chain([go_types_scenario()]) {
        let (base, after) = run(&scenario);
        assert_conservation(&base, &after, scenario.subject);
    }
}

// ── ts: tests/fixtures/ts_findings/default_import_alias (duplicate) and
// tests/fixtures/ts5_findings/known_receiver (relocation) ──────────────────

fn ts_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "ts-dup-step",
            staged: &[
                Staged {
                    src: "tests/fixtures/ts_findings/default_import_alias/generator.ts",
                    dst: "generator.ts",
                },
                Staged {
                    src: "tests/fixtures/ts_findings/default_import_alias/driver.ts",
                    dst: "driver.ts",
                },
            ],
            base: &["generator.ts", "driver.ts"],
            after: &["generator.ts", "driver.ts", "dup.ts"],
            subject: "step",
            mutate: |dir| {
                write_rel(dir, "dup.ts", "export function step(): void {}\n");
            },
        },
        Scenario {
            name: "ts-move-tidy",
            staged: &[
                Staged {
                    src: "tests/fixtures/ts5_findings/known_receiver/consumer.ts",
                    dst: "consumer.ts",
                },
                Staged {
                    src: "tests/fixtures/ts5_findings/known_receiver/ns.ts",
                    dst: "ns.ts",
                },
            ],
            base: &["consumer.ts", "ns.ts"],
            after: &["consumer.ts", "ns.ts", "fresh.ts"],
            subject: "tidy",
            mutate: |dir| {
                replace_in(
                    dir,
                    "consumer.ts",
                    "\n    tidy(text: string): string {\n        return text;\n    }\n",
                    "\n",
                );
                write_rel(
                    dir,
                    "fresh.ts",
                    "export function tidy(text: string): string {\n    return text;\n}\n",
                );
            },
        },
    ]
}

/// FINDING F4: a second free-fn step in dup.ts leaves generator.ts's
/// main -> step edge alive, same dst, origin still corpus_unique. The same
/// shape holds for a duplicated method (this.tidy in known_receiver), so it
/// is not a function-vs-method bucketing artifact.
#[test]
fn duplicate_def_ts() {
    let (base, after) = run(&ts_scenarios()[0]);
    assert_duplicate_def(&base, &after, "step");
}

/// Invariant 2: the tidy method leaves the class for a free fn in fresh.ts;
/// the call site's own text is unedited (the method sat below it), so the row
/// must survive with only callee_path moved.
#[test]
fn relocation_ts() {
    let (base, after) = run(&ts_scenarios()[1]);
    assert_eq!(
        base.calls.len(),
        after.calls.len(),
        "relocation changed the edge count"
    );
    let moved = after
        .calls
        .iter()
        .find(|edge| edge.callee_name.as_deref() == Some("tidy"))
        .expect("relocated def lost its edge");
    assert_eq!(moved.callee_path, "fresh.ts", "edge did not follow the def");
    assert_eq!(moved.origin, CORPUS_UNIQUE);
    let bystanders = |rows: &Rows| -> BTreeSet<CallEdge> {
        rows.calls
            .iter()
            .filter(|edge| edge.callee_name.as_deref() != Some("tidy"))
            .cloned()
            .collect()
    };
    assert_eq!(
        bystanders(&base),
        bystanders(&after),
        "the tidy move disturbed the module_plane normalize edge"
    );
}

/// Invariant 4 over every ts mutation. TS legs mint no `same_file` call
/// edges (a same-file bare call lands on the corpus_unique leg), so the
/// conserved set is empty and this pins that it stays empty.
#[test]
fn origin_conservation_ts() {
    for scenario in ts_scenarios() {
        let (base, after) = run(&scenario);
        assert_conservation(&base, &after, scenario.subject);
    }
}

// ── rust: tests/fixtures/rust_findings/impl_owner + origin/rust_same_file ──

const RUST: &[Staged] = &[
    Staged {
        src: "tests/fixtures/rust_findings/impl_owner/decl.rs",
        dst: "decl.rs",
    },
    Staged {
        src: "tests/fixtures/rust_findings/impl_owner/impls.rs",
        dst: "impls.rs",
    },
    Staged {
        src: "tests/fixtures/rust_findings/impl_owner/lib.rs",
        dst: "lib.rs",
    },
    Staged {
        src: "tests/fixtures/rust_findings/impl_owner/render.rs",
        dst: "render.rs",
    },
    Staged {
        src: "tests/fixtures/origin/rust_same_file.rs",
        dst: "rust_same_file.rs",
    },
];

fn rust_scenario() -> Scenario {
    Scenario {
        name: "rs-move-widget",
        staged: RUST,
        base: &[
            "decl.rs",
            "impls.rs",
            "lib.rs",
            "render.rs",
            "rust_same_file.rs",
        ],
        after: &[
            "decl.rs",
            "impls.rs",
            "lib.rs",
            "render.rs",
            "rust_same_file.rs",
            "transplanted.rs",
        ],
        subject: "Widget",
        mutate: |dir| {
            replace_in(
                dir,
                "decl.rs",
                "pub struct Widget {\n    pub tag: u32,\n}\n\n",
                "",
            );
            write_rel(
                dir,
                "transplanted.rs",
                "pub struct Widget {\n    pub tag: u32,\n}\n",
            );
            replace_in(
                dir,
                "lib.rs",
                "pub mod decl;",
                "pub mod decl;\npub mod transplanted;",
            );
            replace_in(
                dir,
                "impls.rs",
                "use crate::decl::{Holder, Widget};",
                "use crate::decl::Holder;\nuse crate::transplanted::Widget;",
            );
        },
    }
}

/// Invariant 2 on the rust types plane: Widget moves decl.rs ->
/// transplanted.rs, wired through lib.rs and the use line. The use-line
/// rewrite shifts every later owner span in impls.rs, so identity is
/// (owner_name, target_name), never spans.
#[test]
fn relocation_rust_types() {
    let (base, after) = run(&rust_scenario());
    assert_eq!(
        base.types.len(),
        after.types.len(),
        "relocation changed the type edge count"
    );
    let moved: Vec<&TypeEdge> = after.type_edges_to("Widget");
    assert_eq!(
        moved.len(),
        base.type_edges_to("Widget").len(),
        "edges to Widget lost by the move"
    );
    assert!(
        moved
            .iter()
            .all(|edge| edge.target_path == "transplanted.rs"),
        "edge did not follow the def: {moved:?}"
    );
    let bystanders = |rows: &Rows| -> BTreeSet<(Option<String>, Option<String>, String)> {
        rows.types
            .iter()
            .filter(|edge| edge.target_name.as_deref() != Some("Widget"))
            .map(|edge| {
                (
                    edge.owner_name.clone(),
                    edge.target_name.clone(),
                    edge.target_path.clone(),
                )
            })
            .collect()
    };
    assert_eq!(
        bystanders(&base),
        bystanders(&after),
        "the Widget move disturbed the Render/Shade edges"
    );
}

/// Invariant 4 over the rust mutation: rust_same_file.rs's same_file call
/// edge (run -> helper) is a bystander of the Widget move and must survive
/// byte-identical, spans included, because its file is never edited.
#[test]
fn origin_conservation_rust() {
    let (base, after) = run(&rust_scenario());
    assert_conservation(&base, &after, "Widget");
}
