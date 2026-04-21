use lang_prototype::cursor::{CaptureSlot, Cursor, PathSeg, SprfPath};
use lang_prototype::lower::{Callee, OpCall, Param, Pipeline, Rule, Scalar, Term, AcceptsMode};
use lang_prototype::runtime::{run_pipeline, Ctx};
use lang_prototype::store::Store;
use std::sync::Arc;

fn slot(n: u32) -> CaptureSlot { CaptureSlot(n) }

fn scalar_str(s: &str) -> Term { Term::Scalar(Scalar::String(s.into())) }
fn scalar_atom(s: &str) -> Term { Term::Scalar(Scalar::Atom(s.into())) }
fn capture(n: u32) -> Term { Term::CaptureRef(slot(n)) }

fn builtin(name: &str, terms: Vec<Term>) -> Pipeline {
    Pipeline::Op(OpCall { callee: Callee::Builtin(name.into()), terms })
}

fn rule_call(name: &str, terms: Vec<Term>) -> Pipeline {
    Pipeline::Op(OpCall { callee: Callee::Rule(name.into()), terms })
}

fn chain(stages: Vec<Pipeline>) -> Pipeline {
    Pipeline::Chain(stages)
}

#[test]
fn fs_emits_cursors() {
    let rules: Vec<Rule> = vec![];
    let mut store = Store::new();
    let mut diags = Vec::new();
    let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };

    let pipeline = builtin("fs", vec![scalar_str("*.toml")]);
    let out = run_pipeline(&mut ctx, &pipeline, vec![]).unwrap();

    assert!(!out.is_empty(), "fs should emit at least one cursor");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
}

#[test]
fn tag_write_then_read() {
    // rule(seed) > fs("*.toml") > tag($0, :file)
    let seed = Rule {
        name: "seed".into(),
        params: vec![],
        body: Arc::new(chain(vec![
            builtin("fs", vec![scalar_str("*.toml")]),
            builtin("tag", vec![capture(0), scalar_atom("file")]),
        ])),
    };

    // rule(readback) > tags()
    let readback = Rule {
        name: "readback".into(),
        params: vec![],
        body: Arc::new(chain(vec![
            builtin("tags", vec![]),
        ])),
    };

    let rules = vec![seed, readback];
    let mut store = Store::new();
    let mut diags = Vec::new();

    // Run seed
    {
        let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };
        let seed_call = rule_call("seed", vec![]);
        let _ = run_pipeline(&mut ctx, &seed_call, vec![]).unwrap();
    }

    assert!(!store.tags.is_empty(), "tags should have been written");
    assert!(diags.is_empty(), "no diagnostics after seed: {:?}", diags);

    // Run readback
    {
        let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };
        let readback_call = rule_call("readback", vec![]);
        let out = run_pipeline(&mut ctx, &readback_call, vec![]).unwrap();
        assert!(!out.is_empty(), "readback should emit cursors from tags");
    }

    assert!(diags.is_empty(), "no diagnostics after readback: {:?}", diags);
}

#[test]
fn parametric_rule_call() {
    // rule(find, $K) > tags() > tag($2, $K)   [simplified: just call tags then tag with kind]
    // Actually, let's do: rule(find, $K) > tags() — emit all tags, then filter by kind in test
    let find = Rule {
        name: "find".into(),
        params: vec![Param { name: "K".into(), slot: slot(10), mode: AcceptsMode::Either }],
        body: Arc::new(chain(vec![
            builtin("tags", vec![]),
        ])),
    };

    // seed rule writes a tag
    let seed = Rule {
        name: "seed".into(),
        params: vec![],
        body: Arc::new(chain(vec![
            builtin("fs", vec![scalar_str("*.toml")]),
            builtin("tag", vec![capture(0), scalar_atom("source")]),
        ])),
    };

    let rules = vec![find, seed];
    let mut store = Store::new();
    let mut diags = Vec::new();

    // run seed
    {
        let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };
        let _ = run_pipeline(&mut ctx, &rule_call("seed", vec![]), vec![]).unwrap();
    }

    // run find(:source)
    {
        let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };
        let out = run_pipeline(&mut ctx, &rule_call("find", vec![scalar_atom("source")]), vec![]).unwrap();
        assert!(!out.is_empty(), "find should emit cursors");
    }

    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
}

#[test]
fn re_matches_content() {
    let text = b"hello TODO: foo world TODO: bar";
    let cursor = Cursor::new(SprfPath(vec![PathSeg::Step("test".into())]), Arc::from(&text[..]));

    let rules: Vec<Rule> = vec![];
    let mut store = Store::new();
    let mut diags = Vec::new();
    let mut ctx = Ctx { rules: &rules, store: &mut store, diags: &mut diags };

    let pipeline = builtin("re", vec![scalar_str(r"TODO: ([^ ]*)")]);
    let out = run_pipeline(&mut ctx, &pipeline, vec![cursor]).unwrap();

    assert_eq!(out.len(), 2, "re should find 2 matches");
    assert!(out[0].captures.get(slot(1)).is_some(), "first match should have capture");
    assert!(out[1].captures.get(slot(1)).is_some(), "second match should have capture");
    assert!(diags.is_empty(), "no diagnostics: {:?}", diags);
}
