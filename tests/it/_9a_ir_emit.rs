//! Portable IR bundle through the public CLI and the installed RxJS runtime.

use dl8::_6_eval::ir_bundle::Bundle;
use dl8::_6_eval::ir_lower;
use dl8::_6_eval::json::program_from_json;
use dl8::_6_eval::Universe;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

const RUNNER: &str = r#"
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { from, lastValueFrom, toArray } from 'rxjs';

const bundle = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const module = await import(pathToFileURL(process.argv[3]).href);
const batches = await lastValueFrom(module.run(from([bundle.frontier0])).pipe(toArray()));
const dictionary = batches.at(-1)[module.dictionary].byId;
const relations = new Map(bundle.program.rels.map(relation => [relation.id, relation]));
const originals = new Map(Object.values(bundle.atoms).map(entry => [entry.original, entry.printed]));

function decode(cell, type) {
  if (type === 'Int') return cell;
  const initial = bundle.atoms[cell];
  if (initial) return initial.printed;
  const term = dictionary[cell];
  if (!term) throw new Error(`unknown term cell ${cell}`);
  if (term.text !== undefined) return { s: term.text };
  if (term.functor === '0:int') return term.args[0];
  if (term.functor === '1:bool') return term.args[0] !== 0;
  if (term.functor === '3:atom') throw new Error(`unknown atom cell ${cell}`);
  const match = /^9:\d+:(.*)$/.exec(term.functor);
  if (!match) throw new Error(`unknown constructor ${term.functor}`);
  const args = term.args.map((value, index) => decode(value, term.types[index]));
  if (match[1] === '[|]' && Array.isArray(args[1])) return [args[0], ...args[1]];
  return { f: match[1], args };
}

const closure = [];
for (const { rel, row, w } of batches[0]) {
  const relation = relations.get(rel);
  const match = /^__ir_n(\d+)_a\d+$/.exec(relation.name);
  if (!match) continue;
  if (w <= 0) throw new Error(`nonpositive initial output ${relation.name}: ${w}`);
  const name = originals.get(Number(match[1]));
  if (!name) throw new Error(`unknown relation term ${match[1]}`);
  closure.push({ rel: name, args: row.map((cell, index) => decode(cell, relation.cols[index])) });
}
console.log(JSON.stringify({ closure }));
"#;

fn sorted_rows(value: &Value) -> Vec<Value> {
    let mut rows = value["closure"].as_array().unwrap().clone();
    rows.sort_by_key(Value::to_string);
    rows
}

#[test]
fn emitted_rxjs_matches_ir_dd_for_three_ladder_programs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ts = root.join("sqlite_ivm/crates/ivm-rxjs/ts");
    let modules = ts.join("node_modules");
    if !modules.exists() {
        let installed = Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(&ts)
            .output()
            .unwrap();
        assert!(
            installed.status.success(),
            "{}",
            String::from_utf8_lossy(&installed.stderr)
        );
    }
    for stem in ["1_count", "8_deep_terms_order", "12_transitive"] {
        let fixture = root.join(format!("fixtures/ladder/{stem}.json"));
        let source: Value = serde_json::from_slice(&std::fs::read(&fixture).unwrap()).unwrap();
        let mut universe = Universe::new();
        let program = program_from_json(&mut universe, &source).unwrap();
        let lowered = ir_lower::lower(&mut universe, &program)
            .unwrap_or_else(|_| panic!("{stem}: IR lowering"));

        let emitted_json = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["emit", "ir-json"])
            .arg(&fixture)
            .output()
            .unwrap();
        assert!(
            emitted_json.status.success(),
            "{stem}: {}",
            String::from_utf8_lossy(&emitted_json.stderr)
        );
        let bundle: Bundle = serde_json::from_slice(&emitted_json.stdout).unwrap();
        let mut expected_program = lowered.ir;
        expected_program.texts = lowered.texts.iter().map(|(_, text)| text.clone()).collect();
        assert_eq!(bundle.program, expected_program, "{stem}: IR program");
        assert_eq!(bundle.texts, bundle.program.texts, "{stem}: text table");
        assert!(!bundle.frontier0.changes.is_empty(), "{stem}: frontier0");
        assert!(!bundle.atoms.is_empty(), "{stem}: atoms");

        let emitted_rxjs = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["emit", "ir-rxjs"])
            .arg(&fixture)
            .output()
            .unwrap();
        assert!(
            emitted_rxjs.status.success(),
            "{stem}: {}",
            String::from_utf8_lossy(&emitted_rxjs.stderr)
        );
        let directory = root.join(format!("target/ir-emit-{}-{stem}", std::process::id()));
        if directory.exists() {
            std::fs::remove_dir_all(&directory).unwrap();
        }
        std::fs::create_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(&modules, directory.join("node_modules")).unwrap();
        let bundle_path = directory.join("bundle.json");
        let module_path = directory.join("module.ts");
        std::fs::write(&bundle_path, &emitted_json.stdout).unwrap();
        std::fs::write(&module_path, &emitted_rxjs.stdout).unwrap();
        std::fs::write(directory.join("runner.mjs"), RUNNER).unwrap();
        let node = Command::new("node")
            .arg("--experimental-strip-types")
            .arg(directory.join("runner.mjs"))
            .arg(&bundle_path)
            .arg(&module_path)
            .current_dir(&directory)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(
            node.status.success(),
            "{stem}: {}",
            String::from_utf8_lossy(&node.stderr)
        );
        let actual: Value = serde_json::from_slice(&node.stdout).unwrap();

        let dd = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["eval"])
            .arg(&fixture)
            .env("DL8_ENGINE", "ir:dd")
            .output()
            .unwrap();
        assert!(
            dd.status.success(),
            "{stem}: {}",
            String::from_utf8_lossy(&dd.stderr)
        );
        let reference: Value = serde_json::from_slice(&dd.stdout).unwrap();
        assert_eq!(
            reference["diagnostics"],
            json!([]),
            "{stem}: ir:dd diagnostics"
        );
        let actual_rows = sorted_rows(&actual);
        let reference_rows = sorted_rows(&reference);
        if actual_rows != reference_rows {
            let got = actual_rows
                .iter()
                .filter(|row| !reference_rows.contains(row))
                .take(5)
                .collect::<Vec<_>>();
            let missing = reference_rows
                .iter()
                .filter(|row| !actual_rows.contains(row))
                .take(5)
                .collect::<Vec<_>>();
            panic!(
                "{stem}: RxJS closure got={} want={} extra={got:?} missing={missing:?}",
                actual_rows.len(),
                reference_rows.len()
            );
        }
    }
}
