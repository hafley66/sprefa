//! Python front-end parity test: drives `PythonSource` directly. Expected values
//! are hand-derived from `sample.py`, never copied from the extractor's output.

use sprefa_extract::{FamilyMask, PythonSource, Source};

const PATH: &str = "tests/fixtures/python/sample.py";
const SOURCE: &[u8] = include_bytes!("fixtures/python/sample.py");

#[test]
fn python_type_entities_and_sigs() {
    let output = PythonSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();

    let entities: Vec<(&str, Option<&str>, u32)> = types
        .nodes
        .iter()
        .map(|node| {
            (
                node.kind.as_str(),
                node.name.map(|id| output.strings.lookup(id)),
                node.span.start,
            )
        })
        .collect();
    assert_eq!(
        entities,
        [
            ("module", Some("<module>"), 0),
            ("function", Some("add"), 11),
            ("class", Some("Animal"), 71),
            ("method", Some("speak"), 89),
            ("class", Some("Dog"), 136),
            ("method", Some("bark"), 159),
            ("function", Some("inner"), 191),
            ("function", Some("main"), 263),
        ]
    );

    let sigs: Vec<(u32, &str, u32, &str)> = types
        .aux
        .sigs
        .iter()
        .map(|sig| {
            (
                sig.owner.start,
                sig.slot.as_str(),
                sig.pos,
                output.strings.lookup(sig.ty),
            )
        })
        .collect();
    assert_eq!(
        sigs,
        [
            (11, "param", 0, "Number"),
            (11, "param", 1, "Number"),
            (11, "ret", 0, "Number"),
            (89, "ret", 0, "Text"),
            (159, "ret", 0, "Text"),
            (191, "ret", 0, "Text"),
            (263, "ret", 0, "Result"),
        ]
    );
}

#[test]
fn python_call_defs_and_sites() {
    let output = PythonSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let defs: Vec<(&str, Option<&str>)> = call
        .nodes
        .iter()
        .map(|node| {
            (
                node.kind.as_str(),
                node.name.map(|id| output.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(
        defs,
        [
            ("function", Some("add")),
            ("method", Some("speak")),
            ("method", Some("bark")),
            ("function", Some("inner")),
            ("function", Some("main")),
        ]
    );

    let sites: Vec<&str> = call
        .aux
        .sites
        .iter()
        .map(|site| output.strings.lookup(site.callee))
        .collect();
    assert_eq!(sites, ["inner", "add", "Dog", "bark", "join"]);
}

#[test]
fn python_cst_plane_roots_at_module() {
    let output = PythonSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let cst = output.cst.as_ref().unwrap();
    assert!(!cst.nodes.is_empty());
    let root_kind = output.strings.lookup(cst.nodes[0].kind);
    assert_eq!(root_kind, "module");
}

#[test]
fn python_family_mask_leaves_masked_off_families_none() {
    let output = PythonSource.extract(
        PATH,
        SOURCE,
        FamilyMask {
            cst: true,
            types: false,
            call: false,
            df: false,
            data: false,
        },
    );
    assert!(output.cst.is_some());
    assert!(output.types.is_none());
    assert!(output.call.is_none());
    assert!(output.df.is_none());
}

const DOCS_PATH: &str = "tests/fixtures/python/docs.py";
const DOCS_SOURCE: &[u8] = include_bytes!("fixtures/python/docs.py");

/// One row per imported name; the path-only form keeps the path in `name`, an
/// alias moves the path to `module`, a `from` form always sets `module` and
/// sets `imported` only when the local name differs from the source name.
#[test]
fn python_import_specifiers() {
    let output = PythonSource.extract(DOCS_PATH, DOCS_SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();
    let rows: Vec<(&str, &str, Option<&str>, Option<&str>)> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            (
                specifier.kind.as_str(),
                output.strings.lookup(specifier.name),
                specifier.module.map(|id| output.strings.lookup(id)),
                specifier.imported.map(|id| output.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("named", "Optional", Some("typing"), None),
            ("named", "osp", Some("os.path"), None),
            ("named", "sibling", Some("."), None),
            ("named", "alias", Some(".pkg.sub"), Some("thing")),
            ("named", "other", Some(".pkg.sub"), None),
        ]
    );

    let sample = PythonSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let sample_call = sample.call.as_ref().unwrap();
    let plain: Vec<(&str, Option<&str>)> = sample_call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            (
                sample.strings.lookup(specifier.name),
                specifier.module.map(|id| sample.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(plain, [("os", None)]);
}

/// Sphinx field tags off the class and method docstrings: `:param name:` keeps
/// the name as `arg`, `:returns:`/`:return:` normalize to `returns` with no arg.
#[test]
fn python_doc_tags() {
    let output = PythonSource.extract(DOCS_PATH, DOCS_SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();
    let strings = &output.strings;
    let mut tags: Vec<(u32, &str, Option<&str>, &str)> = Vec::new();
    for doc in &types.aux.docs {
        for tag in &doc.tags {
            tags.push((
                doc.owner.start,
                strings.lookup(tag.tag),
                tag.arg.map(|id| strings.lookup(id)),
                strings.lookup(tag.text),
            ));
        }
    }
    assert_eq!(
        tags,
        [
            (0, "author", None, "fixture"),
            (160, "param", Some("name"), "the engine name"),
            (160, "returns", None, "nothing"),
            (305, "param", Some("mode"), "how to run"),
            (305, "returns", None, "the outcome"),
        ]
    );
    let texts: Vec<&str> = types
        .aux
        .docs
        .iter()
        .map(|doc| output.strings.lookup(doc.text).lines().next().unwrap_or(""))
        .collect();
    assert_eq!(
        texts,
        [
            "Module docstring.",
            "An engine.",
            "Run it.",
            "single-quoted doc"
        ]
    );
}
