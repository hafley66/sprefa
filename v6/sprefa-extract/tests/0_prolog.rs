use sprefa_extract::{
    build_def_index, flatten, BlobHash, CallEdgeKind, CallF, FamilyMask, FamilyTag, FileSet,
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
    let blob = BlobHash::of(SOURCE);
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
    let errors: Vec<_> = files
        .iter()
        .filter_map(|path| {
            let source = std::fs::read(path).unwrap();
            let tree = parser.parse(&source, None).unwrap();
            tree.root_node()
                .has_error()
                .then(|| path.strip_prefix(corpus).unwrap().display().to_string())
        })
        .collect();

    assert_eq!(files.len(), 53);
    assert_eq!(
        errors,
        [
            "0_enum_expand.pl",
            "0_match_expand.pl",
            "0_program_check.pl",
            "1_host_expand.pl",
            "compile/analyze.pl",
            "compile/compile.pl",
            "compile/emit_ts.pl",
            "compile/lower.pl",
            "compile/parse_dl.pl",
            "compile/print_dl.pl",
            "compile/strat.pl",
            "compile/sweep.pl",
            "compile/test/plunit_tests.pl",
            "compile/test/run_sql_check.pl",
            "conformance/body.pl",
            "conformance/engine.pl",
            "conformance/fixtures/0_enum_variants.pl",
            "conformance/fixtures/1_match_block.pl",
            "conformance/fixtures/2_hosts_wiring.pl",
            "conformance/fixtures/3_flagship_callgraph.pl",
            "conformance/fixtures/check_eventing.pl",
            "conformance/fixtures/engine_core.pl",
            "conformance/fixtures/expressions.pl",
            "conformance/fixtures/json_arm.pl",
            "conformance/fixtures/merge_family.pl",
            "conformance/fixtures/occurrence_identity.pl",
            "conformance/fixtures/operators.pl",
            "conformance/fixtures/scopes.pl",
            "conformance/fixtures/shell_stream.pl",
            "conformance/fixtures/spine_semantics.pl",
            "conformance/fixtures/state_machine.pl",
            "conformance/fixtures/temporal_pipe.pl",
            "conformance/fixtures/timeless_rail.pl",
            "conformance/go.pl",
            "conformance/level_eval.pl",
            "conformance/ticklog.pl",
            "examples/ghcacher.pl",
            "src/checks.pl",
            "src/emit_ts.pl",
        ]
    );
}

fn collect_prolog_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_prolog_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "pl") {
            files.push(path);
        }
    }
}
