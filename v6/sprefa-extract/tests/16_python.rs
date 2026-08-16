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
        },
    );
    assert!(output.cst.is_some());
    assert!(output.types.is_none());
    assert!(output.call.is_none());
    assert!(output.df.is_none());
}
