use sprefa_extract::{
    build_def_index, content_id_of, flatten, CallEdgeKind, CallF, FamilyMask, FamilyTag, FileSet,
    IndexBag, ManifestMap, ProjectCx, ProjectDigest, PrologSource, Resolve, Source,
};

const PATH: &str = "tests/fixtures/prolog/0_sample.pl";
const SOURCE: &[u8] = include_bytes!("fixtures/prolog/0_sample.pl");

#[test]
fn prolog_all_families_and_names() {
    let output = PrologSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let facts = flatten(&output);

    let definitions: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Node {
                family: FamilyTag::Call,
                name: Some(name),
                ..
            } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let sites: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Site {
                callee,
                callee_path,
                ..
            } => Some((callee.as_str(), callee_path.as_deref())),
            _ => None,
        })
        .collect();

    assert_eq!(
        definitions,
        [
            "edge/2",
            "edge/2",
            "path/2",
            "path/2",
            "greeting//0",
            "qualified/1"
        ]
    );
    assert_eq!(
        sites,
        [
            ("edge/2", None),
            ("path/2", None),
            ("edge/2", None),
            ("token//1", None),
            ("token//1", None),
            ("member/2", Some("lists:member/2")),
        ]
    );
    assert!(output
        .cst
        .as_ref()
        .is_some_and(|bundle| !bundle.nodes.is_empty()));
    assert!(output
        .df
        .as_ref()
        .is_some_and(|bundle| { !bundle.nodes.is_empty() && !bundle.edges.is_empty() }));
}

#[test]
fn prolog_name_arity_resolution() {
    let output = PrologSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let blob = content_id_of(SOURCE);
    let index = build_def_index(&[(blob, &output)]);
    let indexes = IndexBag::default();
    indexes.def_index.set(index).unwrap();
    let files = FileSet;
    let manifests = ManifestMap;
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: None,
        digest: ProjectDigest::default(),
        indexes,
    };
    let edges = Resolve::<CallF>::resolve(&PrologSource, &output, &cx);
    let resolved: Vec<_> = edges.iter().map(|edge| edge.kind).collect();

    assert_eq!(
        resolved,
        [
            CallEdgeKind::NameResolve,
            CallEdgeKind::NameResolve,
            CallEdgeKind::NameResolve,
        ]
    );
}

/// Only its own job: catch a mis-rooted or empty walk. It is NOT a pin on how
/// many prolog files v6 has. The previous shape asserted an exact corpus size
/// and rotted three times (53 -> 56 -> red at 92) because it was reading a
/// number nothing depends on.
const CORPUS_FLOOR: usize = 60;

/// The ratchet, and the only durable fact in this ledger: these v6 prolog files
/// parse with ZERO error nodes under tree-sitter-prolog today, so `PrologSource`
/// sees a complete tree for them. A file here that starts erroring is a grammar
/// or dependency regression and this list is what catches it. Adding new prolog
/// files never touches this list; renaming or deleting one does, loudly, by the
/// existence check below.
///
/// The complement is deliberately NOT pinned. 52 of the 71 non-lab corpus files
/// carry error nodes (tree-sitter-prolog does not cover the SWI/DCG surface
/// v6/prolog is written in), and `conformance/fixtures/` grows on nearly every
/// arc, so pinning the failing set would go red on additions that say nothing
/// about the grammar. Recovery over those files is asserted directly instead:
/// every file must still yield a tree.
const PARSES_CLEAN: &[&str] = &[
    "0_body_walk.pl",
    "0_unsupported_messages.pl",
    "1_expansion.pl",
    "ARCH.pl",
    "compile/1_emit_registry_docs.pl",
    "compile/2_emit_cli_inventory.pl",
    "6_profile.pl",
    "compile/oracle_dump.pl",
    "compile/registry.pl",
    "compile/scripts/bop_check.pl",
    "compile/scripts/dl6_oracle.pl",
    "compile/scripts/golden_coverage.pl",
    "compile/scripts/golden_oracle.pl",
    "compile/scripts/text_door_receipt.pl",
    "conformance/rulings.pl",
    "src/grader.pl",
    "src/kernel.pl",
    "tools/arch_map.pl",
    "tools/prolog_lint.pl",
];

/// The ledger this test keeps has three parts, each stated as its own assertion
/// so a failure names which one broke:
///  1. RECOVERY: tree-sitter-prolog returns a tree for every corpus file. This
///     is the claim the test name makes and it holds over erroring files too.
///  2. WALK SANITY: the corpus root resolved and was non-trivially populated.
///  3. NO GRAMMAR REGRESSION: every file in `PARSES_CLEAN` still parses clean.
///
/// `labs/` is excluded from the walk. Lab files are transient by the standing
/// lab protocol (labs die on landing, git history is their archive), so their
/// parse status is not a durable fact and their churn is what rotted the old
/// exact-count assertion.
///
/// SABOTAGE RECEIPTS (both run, both red, then reverted):
///  - listing `compile/lower.pl` (a known-erroring file) in `PARSES_CLEAN`
///    -> "now carry error nodes: [\"compile/lower.pl\"]".
///  - listing `compile/renamed_away.pl` (no such file)
///    -> "PARSES_CLEAN names files the corpus no longer has".
#[test]
fn prolog_parser_error_recovery_ledger_for_the_v6_corpus() {
    let corpus = std::path::Path::new("../prolog");
    let mut files = Vec::new();
    collect_prolog_files(corpus, &mut files);
    files.sort();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE))
        .unwrap();

    let mut clean: Vec<String> = Vec::new();
    let mut erroring = 0usize;
    for path in &files {
        let relative = path.strip_prefix(corpus).unwrap().display().to_string();
        let source = std::fs::read(path).unwrap();
        // 1. RECOVERY: a tree comes back for every file, erroring or not.
        let tree = parser.parse(&source, None).unwrap_or_else(|| {
            panic!("tree-sitter-prolog returned no tree for {relative}: recovery failed")
        });
        if tree.root_node().has_error() {
            erroring += 1;
        } else {
            clean.push(relative);
        }
    }

    // 2. WALK SANITY.
    assert!(
        files.len() >= CORPUS_FLOOR,
        "walked only {} prolog files under {}: the corpus root looks wrong, \
         not the corpus (floor {CORPUS_FLOOR} guards the walk, never the size)",
        files.len(),
        corpus.display(),
    );
    assert_eq!(
        files.len(),
        clean.len() + erroring,
        "every walked file is classified exactly once"
    );

    // 3. NO GRAMMAR REGRESSION, both directions of maintenance made loud.
    let walked: Vec<&str> = files
        .iter()
        .map(|path| path.strip_prefix(corpus).unwrap().to_str().unwrap())
        .collect();
    let missing: Vec<&str> = PARSES_CLEAN
        .iter()
        .copied()
        .filter(|listed| !walked.contains(listed))
        .collect();
    assert!(
        missing.is_empty(),
        "PARSES_CLEAN names files the corpus no longer has: {missing:?}. \
         They were renamed or deleted; update the list."
    );
    let regressed: Vec<&str> = PARSES_CLEAN
        .iter()
        .copied()
        .filter(|listed| !clean.iter().any(|got| got == listed))
        .collect();
    assert!(
        regressed.is_empty(),
        "these files parsed clean when this ledger was written and now carry \
         error nodes: {regressed:?}. tree-sitter-prolog regressed, or the file \
         gained syntax the grammar does not cover."
    );
}

/// The corpus walk. `labs/` is skipped: see the ledger test's header.
fn collect_prolog_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "labs") {
                continue;
            }
            collect_prolog_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "pl") {
            files.push(path);
        }
    }
}

/// SABOTAGE RECEIPT: dropping the `[reachable/2, walk//1]` list from the
/// fixture's second `use_module` leaves one side_effect row and no named rows,
/// and this test fails on the missing `graph:reachable/2` pair.
#[test]
fn prolog_module_declaration_and_import_lists() {
    let output = PrologSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let facts = flatten(&output);

    let specifiers: Vec<(&str, Option<&str>, &str)> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Specifier {
                kind, module, name, ..
            } => Some((kind.as_str(), module.as_deref(), name.as_str())),
            _ => None,
        })
        .collect();

    assert_eq!(
        specifiers,
        [
            ("reexport", Some("sample"), "path/2"),
            ("reexport", Some("sample"), "greeting//0"),
            ("side_effect", None, "library(lists)"),
            ("named", Some("'../shared/graph'"), "reachable/2"),
            ("named", Some("'../shared/graph'"), "walk//1"),
        ]
    );
}

/// The re-home program keys on the include/reexport rows, so this pins their
/// exact spelling: `include/1` rides `name` with no module, `reexport/1` rides
/// `name` with no module, and `reexport/2` keys on (module, name) one row per
/// indicator, mirroring the module-import two-argument form.
#[test]
fn prolog_include_and_reexport_directives() {
    let source: &[u8] = b"\
:- module(rehome, []).
:- use_module(library(lists)).
:- include('part.pl').
:- reexport('other.pl').
:- reexport('third.pl', [foo/1, bar//0]).
hello.
";
    let output = PrologSource.extract(PATH, source, FamilyMask::ALL);
    let facts = flatten(&output);

    let specifiers: Vec<(&str, Option<&str>, &str)> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Specifier {
                kind, module, name, ..
            } => Some((kind.as_str(), module.as_deref(), name.as_str())),
            _ => None,
        })
        .collect();

    assert_eq!(
        specifiers,
        [
            ("side_effect", None, "library(lists)"),
            ("include", None, "'part.pl'"),
            ("reexport_module", None, "'other.pl'"),
            ("reexport_module", Some("'third.pl'"), "foo/1"),
            ("reexport_module", Some("'third.pl'"), "bar//0"),
        ]
    );
}

/// FAIL PRE FIX: `use_module('part', [])` produced zero specifier rows, so
/// `extract move` never rewrote `1_expansion.pl:28` and the moved module was
/// unreachable (lab/rehome-passes move 8, 2026-08-24). An empty import list
/// is a load edge and rides `side_effect` like the one-argument form.
#[test]
fn prolog_empty_import_list_is_a_side_effect_load() {
    let source: &[u8] = b"\
:- module(empty_list, []).
:- use_module('part', []).
:- use_module(other).
hello.
";
    let output = PrologSource.extract(PATH, source, FamilyMask::ALL);
    let facts = flatten(&output);
    let specifiers: Vec<(&str, Option<&str>, &str)> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Specifier {
                kind, module, name, ..
            } => Some((kind.as_str(), module.as_deref(), name.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        specifiers,
        [
            ("side_effect", None, "'part'"),
            ("side_effect", None, "other"),
        ]
    );
}
