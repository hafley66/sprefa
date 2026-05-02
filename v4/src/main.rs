// sprefa v4 prototype — min DD wiring + IR-as-data, all 7 layers, fs + ast-grep.
//
// Hardcoded "script" expressed as OpIR values (see build_ir below):
//   in   ──filter(NameLenGte 8)──►  long
//   in   ──subtract by Name on long──►  short
//   long  ──print──►  stdout
//   short ──print──►  stdout
//
// L0 SOURCE       — InputEvent vec, drives InputSession (insert / remove)
// L1 PARSE+LOWER  — build_ir() returns Vec<OpIR>; lower() walks it, building DD wiring
// L2 RULE STORE   — DD's traces ARE the per-collection store (memo + retract)
// L3 SLICE RUNTIME — OpIR variants ≡ ops; lower() = single match → DD method calls
// L4 SAGA DISPATCH — Print variant emits inspect_batch sinks
// L5 SUBSTRATE    — timely worker + DD scope; FsSubstrate for filesystem
// L6 OS           — std::fs

use std::collections::HashMap;
use std::path::PathBuf;

use ast_grep_core::{source::StrDoc, AstGrep, Pattern};
use ast_grep_language::SupportLang;

use differential_dataflow::input::InputSession;
use differential_dataflow::operators::{Join, Threshold};
use differential_dataflow::Collection;
use timely::dataflow::operators::probe::Handle;
use timely::dataflow::Scope;

// ─────────────────────────────────────────────────────────── shared types ──

type Gen = u64;

/// Cursor row. Tiny + Hash/Ord/Clone-friendly.
type Cursor = (
    String, // path
    u32,    // lo
    u32,    // hi
    String, // capture: NAME (empty if none)
);

// ─────────────────────────────────────────────────────────── L3 IR ──
// Ops are first-class IR values. Same shape as a Redux "action creator
// graph": each entry derives a named fact from prior facts.

#[derive(Debug, Clone)]
enum OpIR {
    /// Filter `src` by predicate, write to `out`.
    Filter   { src: String, pred: Pred, out: String },
    /// `out` = `left` minus rows whose `key` appears in `right`'s `key`s.
    /// (Slice over antijoin: project key, distinct, antijoin, project back.)
    Subtract { left: String, right: String, key: KeyFn, out: String },
    /// Sink: drain `src` to stdout, prefixed with `label`.
    Print    { src: String, label: String },
}

#[derive(Debug, Clone)]
enum Pred  { NameLenGte(usize) }
#[derive(Debug, Clone, Copy)]
enum KeyFn { Name }

fn pred_eval(p: &Pred, c: &Cursor) -> bool {
    match p { Pred::NameLenGte(n) => c.3.len() >= *n }
}
fn key_of(k: KeyFn, c: &Cursor) -> String {
    match k { KeyFn::Name => c.3.clone() }
}

// L1 builder: hand-built IR for the demo. A real lowerer takes parsed
// .sprf source. Same enum shape either way.
fn build_ir() -> Vec<OpIR> {
    vec![
        OpIR::Filter   { src: "in".into(),    pred: Pred::NameLenGte(8),
                         out: "long".into() },
        OpIR::Subtract { left: "in".into(),   right: "long".into(),
                         key: KeyFn::Name,
                         out: "short".into() },
        OpIR::Print    { src: "long".into(),  label: "long ".into() },
        OpIR::Print    { src: "short".into(), label: "short".into() },
    ]
}

// L3 lowerer: walks IR, threads named Collections through a HashMap.
// Single match arm per variant — every "op" in the language is one DD
// method-call recipe. Adding a new op = adding a variant + an arm.
fn lower<G: Scope<Timestamp = Gen>>(
    ir:    &[OpIR],
    input: Collection<G, Cursor>,
    probe: &mut Handle<Gen>,
) {
    let mut facts: HashMap<String, Collection<G, Cursor>> = HashMap::new();
    facts.insert("in".into(), input);

    for op in ir {
        match op {
            OpIR::Filter { src, pred, out } => {
                let s = facts.get(src).expect("undefined src in Filter").clone();
                let pred = pred.clone();
                let derived = s.filter(move |c| pred_eval(&pred, c));
                facts.insert(out.clone(), derived);
            }
            OpIR::Subtract { left, right, key, out } => {
                let l = facts.get(left ).expect("undefined left in Subtract").clone();
                let r = facts.get(right).expect("undefined right in Subtract").clone();
                let k = *key;
                let l_kv      = l.map(move |c| (key_of(k, &c), c));
                let right_keys = r.map(move |c| key_of(k, &c)).distinct();
                let kept      = l_kv.antijoin(&right_keys).map(|(_, c)| c);
                facts.insert(out.clone(), kept);
            }
            OpIR::Print { src, label } => {
                let s = facts.get(src).expect("undefined src in Print").clone();
                let label = label.clone();
                s.inspect_batch(move |t, batch| {
                    for ((path, lo, hi, name), _, diff) in batch {
                        let sign = if *diff > 0 { '+' } else { '-' };
                        println!(
                            "[{}] {} gen={} {}:{}..{}  fn {}",
                            sign, label, t, path, lo, hi, name
                        );
                    }
                }).probe_with(probe);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────── L6/L5 substrate ──

fn walk_rs(root: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().map_or(false, |n| n == "target" || n.to_string_lossy().starts_with('.')) {
                continue;
            }
            walk_rs(&p, out);
        } else if p.extension().map_or(false, |x| x == "rs") {
            out.push(p);
        }
    }
}

// ─────────────────────────────────────────────────────────── L1 input seam ──

fn extract_fns(path: &PathBuf, pattern: &Pattern<SupportLang>) -> Vec<Cursor> {
    let Ok(src) = std::fs::read_to_string(path) else { return vec![] };
    let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(&src, SupportLang::Rust);
    let mut out = Vec::new();
    let path_s = path.display().to_string();
    for nm in grep.root().find_all(pattern) {
        let r = nm.range();
        let env = nm.get_env();
        let name = env.get_match("NAME").map(|n| n.text().to_string()).unwrap_or_default();
        out.push((path_s.clone(), r.start as u32, r.end as u32, name));
    }
    out
}

// ─────────────────────────────────────────────────────────── L0 events ──

enum InputEvent {
    Run    { root: PathBuf },
    Forget { path: String },
}

// ─────────────────────────────────────────────────────────── main ──

fn main() {
    let root = std::env::args().nth(1)
        .map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

    let events = vec![
        InputEvent::Run    { root: root.clone() },
        InputEvent::Forget { path: format!("{}/src/main.rs", root.display()) },
    ];

    timely::execute_directly(move |worker| {
        let mut input: InputSession<Gen, Cursor, isize> = InputSession::new();
        let mut probe = Handle::new();
        let ir = build_ir();

        worker.dataflow::<Gen, _, _>(|scope| {
            let cursors = input.to_collection(scope);
            lower(&ir, cursors, &mut probe);
        });

        let pat = Pattern::new("fn $NAME", SupportLang::Rust);
        let mut by_path: HashMap<String, Vec<Cursor>> = Default::default();

        let mut t: Gen = 0;
        for ev in events {
            t += 1;
            match ev {
                InputEvent::Run { root } => {
                    let mut files = Vec::new();
                    walk_rs(&root, &mut files);
                    for f in files {
                        let rows = extract_fns(&f, &pat);
                        for r in &rows { input.insert(r.clone()); }
                        by_path.insert(f.display().to_string(), rows);
                    }
                }
                InputEvent::Forget { path } => {
                    if let Some(rows) = by_path.remove(&path) {
                        for r in rows { input.remove(r); }
                    } else {
                        eprintln!("(forget: no rows seen for {})", path);
                    }
                }
            }
            input.advance_to(t + 1);
            input.flush();
            worker.step_while(|| probe.less_than(input.time()));
            eprintln!("── gen {} drained.", t);
        }
    });
}
