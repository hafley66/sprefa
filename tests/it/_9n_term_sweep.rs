//! `OneEngine::term_sweep`: twenty programs, each with a new relation, compiled on one universe
//! and one process engine with the sweep on and off. Every compile's output is equal, the sweep
//! frees terms, and the live term count after compile 20 stays within 10% of compile 2's.

use dl8::_6_eval::Universe;
use dl8::_8_driver::_7_one::OneEngine;
use std::path::Path;

fn shown(u: &Universe, c: &dl8::Compile) -> (Vec<String>, String, Vec<String>) {
    let mut rows: Vec<String> = c.compiler_rows.iter().map(|r| u.display(*r).to_string()).collect();
    rows.sort();
    let diagnostics = c.diagnostics.iter().map(|d| u.display(*d).to_string()).collect();
    (rows, u.display(c.runtime_program).to_string(), diagnostics)
}

#[test]
fn twenty_programs_sweep_to_a_flat_universe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::env::set_current_dir(root).unwrap();
    let scratch = std::env::temp_dir().join(format!("dl8-term-sweep-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let path = scratch.join("p.dl7");
    let (mut swept, mut kept) = (Universe::new(), Universe::new());
    let (mut on, mut off) = (OneEngine::new(), OneEngine::new());
    on.term_sweep = true;
    off.term_sweep = false;
    let mut live = Vec::new();
    for i in 0..20 {
        std::fs::write(
            &path,
            format!("(: Greeting (* (: text str)))\n(Greeting \"hello world {i}\")\n(: Count{i} (* (: n int)))\n(Count{i} {i})\n"),
        )
        .unwrap();
        let a = dl8::compile_in(&mut swept, &mut on, &path, &mut |_| {}).unwrap();
        let b = dl8::compile_in(&mut kept, &mut off, &path, &mut |_| {}).unwrap();
        assert_eq!(shown(&swept, &a), shown(&kept, &b), "compile {i}");
        live.push(swept.iter().count());
    }
    std::fs::remove_dir_all(&scratch).unwrap();
    assert!(swept.iter().count() < swept.len(), "the sweep freed no term");
    assert_eq!(kept.iter().count(), kept.len());
    let (second, last) = (live[1] as f64, live[19] as f64);
    assert!(last <= second * 1.1, "live terms: compile 2 {second}, compile 20 {last}");
}
