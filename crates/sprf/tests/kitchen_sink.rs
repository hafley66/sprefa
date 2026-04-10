//! Kitchen sink: ONE test that runs every rule in kitchen_sink.sprf through the
//! full RuleExtractor.extract() pipeline against a synthetic repo of fixture
//! files and snapshots the resulting RawRef records, grouped by rule.
//!
//! If any rule silently stops producing refs, the snapshot changes. That is the
//! whole point: one signal for "nothing works" vs "everything works".

use std::collections::{BTreeMap, BTreeSet};

use sprefa_extract::{ExtractContext, Extractor};
use sprefa_rules::extractor::RuleExtractor;
use sprefa_sprf::parse_sprf;

const FIXTURE_SPRF: &str = include_str!("fixtures/kitchen_sink.sprf");

const CARGO_TOML: &[u8] = include_bytes!("fixtures/cargo.toml");
const PACKAGE_JSON: &[u8] = include_bytes!("fixtures/package.json");
const VALUES_YAML: &[u8] = include_bytes!("fixtures/values.yaml");
const DEPLOY_YAML: &[u8] = include_bytes!("fixtures/deploy.yaml");
const SERVICES_YAML: &[u8] = include_bytes!("fixtures/services.yaml");
const CONFIG_JSON: &[u8] = include_bytes!("fixtures/config.json");
const DATA_JSON: &[u8] = include_bytes!("fixtures/data.json");
const PACKAGE_LOCK_JSON: &[u8] = include_bytes!("fixtures/package-lock.json");
const CARGO_LOCK: &[u8] = include_bytes!("fixtures/Cargo.lock");
const DOCKERFILE: &[u8] = include_bytes!("fixtures/Dockerfile");
const GO_MOD: &[u8] = include_bytes!("fixtures/go.mod");
const REQUIREMENTS_TXT: &[u8] = include_bytes!("fixtures/requirements.txt");

// Synthetic inline fixtures for files the fixtures/ directory doesn't carry.
const CONFIG_YAML: &[u8] = b"version: \"3.5.0\"\n";
const MAKEFILE: &[u8] = b"include common.mk\ninclude build/rules.mk\n";
const PACKAGES_AUTH_VALUES_YAML: &[u8] = b"name: auth-service\nversion: 1.0.0\n";
const README_MD: &[u8] = b"\
# fixture-app

## Installation
npm install express
npm install lodash

## Dependencies
- express
- lodash

## Usage
npm install chalk

See [docs](https://example.com) and [source](https://github.com/x).

```rust
fn main() {}
```

```bash
echo hi
```
";
const LIB_RS: &[u8] = b"\
// SECTION: imports
fn parse() {}
fn lex() {}

fn to_u32() -> u32 { 42 }
fn greet(name: &str) -> String { format!(\"hi {}\", name) }

// BEGIN: auth
fn login() {}
fn logout() {}
// END: auth

fn also_outside() {}

// SECTION: eval
fn evaluate() {}
";
const HANDLERS_TS: &[u8] = b"\
import express from \"express\";
const key = process.env.API_KEY;
const url = process.env.DATABASE_URL;
";
const INDEX_TS: &[u8] = b"export const WIDGET_NAME = \"widget\";\n";
const USER_PANEL_TSX: &[u8] = b"\
import { useUserQuery } from \"./queries\";
const UserPanel = () => { const { data } = useUserQuery({ id: 1 }); return null; };
";

/// Virtual layout: (path, bytes). Each file is extracted independently with the
/// same git context below. Paths are chosen to satisfy the fixture's file selectors.
fn virtual_files() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("Cargo.toml", CARGO_TOML),
        ("package.json", PACKAGE_JSON),
        ("charts/app/values.yaml", VALUES_YAML),
        ("packages/auth/values.yaml", PACKAGES_AUTH_VALUES_YAML),
        ("deploy/deploy.yaml", DEPLOY_YAML),
        ("deploy/services.yaml", SERVICES_YAML),
        ("deploy/config.yaml", CONFIG_YAML),
        ("config.json", CONFIG_JSON),
        ("data.json", DATA_JSON),
        ("package-lock.json", PACKAGE_LOCK_JSON),
        ("Cargo.lock", CARGO_LOCK),
        ("app/Dockerfile", DOCKERFILE),
        ("services/api/go.mod", GO_MOD),
        ("services/api/requirements.txt", REQUIREMENTS_TXT),
        ("README.md", README_MD),
        ("Makefile", MAKEFILE),
        ("src/lib.rs", LIB_RS),
        ("src/components/widget/index.ts", INDEX_TS),
        ("src/handlers/api.ts", HANDLERS_TS),
        ("src/hooks/UserPanel.tsx", USER_PANEL_TSX),
    ]
}

#[test]
fn kitchen_sink_everything_works() {
    let (ruleset, _) = parse_sprf(FIXTURE_SPRF).expect("parse kitchen_sink.sprf");
    let ext = RuleExtractor::from_ruleset(&ruleset).expect("compile RuleExtractor");

    let ctx = ExtractContext {
        repo: Some("myorg/auth-service"),
        branch: Some("main"),
        tags: &["v1.2.3"],
    };

    // rule_name -> sorted list of "path :: KIND = value [scan=...]" lines.
    let mut by_rule: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (path, source) in virtual_files() {
        let refs = ext.extract(source, path, &ctx);
        for r in refs {
            let scan_suffix = r
                .scan
                .as_ref()
                .map(|s| format!(" scan={s}"))
                .unwrap_or_default();
            let line = format!("{path} :: {} = {}{scan_suffix}", r.kind, r.value);
            by_rule.entry(r.rule_name.clone()).or_default().insert(line);
        }
    }

    // Include every declared rule name, even ones that produced nothing, so
    // silent regressions show up in the snapshot as a flip from refs -> empty.
    let declared: BTreeSet<String> = ruleset.rules.iter().map(|r| r.name.clone()).collect();

    let mut out = String::new();
    for name in &declared {
        let refs = by_rule.get(name);
        match refs {
            Some(set) if !set.is_empty() => {
                out.push_str(name);
                out.push_str(":\n");
                for line in set {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            _ => {
                out.push_str(name);
                out.push_str(": (no refs)\n");
            }
        }
    }

    insta::assert_snapshot!(out);
}
