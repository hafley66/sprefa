//! Rule-definition hashes for sprf_meta cache (sprefa-upb Phase A).
//!
//! Two hashes per rule, keyed by name:
//!   * `schema_hash`  — column shape. Trips when the rule's param list
//!                      changes (or any other input affecting table shape).
//!                      Schema change forces DROP+CREATE.
//!   * `extract_hash` — body identity. Trips when the rule body source
//!                      changes, or any predecessor rule's `extract_hash`
//!                      changes (folded via cross-rule call edges).
//!                      Extract-only change forces DELETE+INSERT, schema
//!                      stable.
//!
//! Edges come from `RuleDef::body_op_names` ∩ rule names. The walker
//! lives in `rule_def`; hashing is consumer-side and depends on no Op
//! trait change (Phase A constraint).
//!
//! Hash function: blake3 (Phase 0 architecture lock). Hex-encoded for
//! sqlite TEXT storage (matches v1 sprf_meta column type).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::rule_def::RuleDef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleHashes {
    pub schema_hash:  String,
    pub extract_hash: String,
}

/// Topo-sort + fold. Returns hashes for every rule in `defs`. Cycles
/// degrade to insertion-order; cycle detection is sprefa-zcz's job.
pub fn compute(
    defs:   &HashMap<Arc<str>, RuleDef>,
    source: &[u8],
) -> HashMap<Arc<str>, RuleHashes> {
    let names: HashSet<Arc<str>> = defs.keys().cloned().collect();
    let mut producers: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
    let mut in_deg:    HashMap<Arc<str>, usize>          = HashMap::new();
    for n in &names {
        in_deg.insert(n.clone(), 0);
        producers.insert(n.clone(), Vec::new());
    }
    let mut dependents: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::new();
    for (consumer, def) in defs {
        for op_name in &def.body_op_names {
            if op_name == consumer { continue; }
            if !names.contains(op_name) { continue; }
            producers.get_mut(consumer).unwrap().push(op_name.clone());
            dependents.entry(op_name.clone()).or_default().push(consumer.clone());
            *in_deg.get_mut(consumer).unwrap() += 1;
        }
    }

    let mut sorted: Vec<Arc<str>> = Vec::with_capacity(names.len());
    let mut queue: Vec<Arc<str>> = in_deg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    queue.sort();
    while let Some(n) = queue.pop() {
        sorted.push(n.clone());
        if let Some(ds) = dependents.get(&n) {
            for d in ds {
                let v = in_deg.get_mut(d).unwrap();
                *v -= 1;
                if *v == 0 { queue.push(d.clone()); }
            }
        }
    }
    // Cycle fallback: any unsorted rules get appended in insertion order
    // so they still receive a hash. Cross-rule cycle handling is zcz.
    if sorted.len() < names.len() {
        for n in defs.keys() {
            if !sorted.contains(n) { sorted.push(n.clone()); }
        }
    }

    let mut out: HashMap<Arc<str>, RuleHashes> = HashMap::with_capacity(names.len());
    for name in &sorted {
        let def = defs.get(name).unwrap();
        let schema_hash = schema_hash(def);
        let extract_hash = extract_hash(def, source, producers.get(name).unwrap(), &out);
        out.insert(name.clone(), RuleHashes { schema_hash, extract_hash });
    }
    out
}

fn schema_hash(def: &RuleDef) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"schema\0");
    h.update(def.name.as_bytes());
    h.update(b"\0params=");
    h.update(&(def.params.len() as u32).to_le_bytes());
    for p in &def.params {
        h.update(&(p.len() as u32).to_le_bytes());
        h.update(p.as_bytes());
    }
    h.finalize().to_hex().to_string()
}

fn extract_hash(
    def:       &RuleDef,
    source:    &[u8],
    producers: &[Arc<str>],
    computed:  &HashMap<Arc<str>, RuleHashes>,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"extract\0");
    h.update(def.name.as_bytes());
    h.update(b"\0body=");
    if let Some(r) = def.brace_range.clone() {
        if r.end <= source.len() {
            let body = &source[r];
            h.update(&(body.len() as u32).to_le_bytes());
            h.update(body);
        } else {
            h.update(&0u32.to_le_bytes());
        }
    } else {
        h.update(&0u32.to_le_bytes());
    }
    let mut producers_sorted: Vec<&Arc<str>> = producers.iter().collect();
    producers_sorted.sort();
    h.update(b"\0deps=");
    for p in producers_sorted {
        if let Some(prev) = computed.get(p) {
            h.update(&(p.len() as u32).to_le_bytes());
            h.update(p.as_bytes());
            h.update(prev.extract_hash.as_bytes());
        }
    }
    h.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use sprefa_parse::host_parse_with_injections;
    use std::path::Path;

    fn collect_hashes(src: &str) -> HashMap<Arc<str>, RuleHashes> {
        let registry = Registry::with_stdlib();
        let file: Arc<Path> = Arc::from(Path::new("test.sprf"));
        let (parsed, _errs) = host_parse_with_injections(
            src, file.clone(), &crate::op_languages::language_of,
        );
        let defs = crate::rule_def::collect(&parsed, src.as_bytes(), &registry, &file);
        compute(&defs, src.as_bytes())
    }

    #[test]
    fn rule_hash_basics() {
        // (a) stable across re-runs
        let s = r#"rule(:a, ${X?}) { void() };"#;
        let h1 = collect_hashes(s);
        let h2 = collect_hashes(s);
        assert_eq!(h1, h2);

        // (b) param change trips schema_hash
        let h_a = collect_hashes(r#"rule(:r, ${X?}) { void() };"#);
        let h_b = collect_hashes(r#"rule(:r, ${X?}, ${Y?}) { void() };"#);
        let a_name: Arc<str> = Arc::from("r");
        assert_ne!(h_a[&a_name].schema_hash, h_b[&a_name].schema_hash);

        // (c) body change trips extract_hash, schema stable
        let h_c = collect_hashes(r#"rule(:r, ${X?}) { void() };"#);
        let h_d = collect_hashes(r#"rule(:r, ${X?}) { void() > void() };"#);
        let r: Arc<str> = Arc::from("r");
        assert_eq!(h_c[&r].schema_hash,  h_d[&r].schema_hash);
        assert_ne!(h_c[&r].extract_hash, h_d[&r].extract_hash);

        // (d) producer extract change folds into consumer
        let s_e = r#"
            rule(:p, ${X?}) { void() };
            rule(:c, ${Y?}) { p(${Y?}) };
        "#;
        let s_f = r#"
            rule(:p, ${X?}) { void() > void() };
            rule(:c, ${Y?}) { p(${Y?}) };
        "#;
        let h_e = collect_hashes(s_e);
        let h_f = collect_hashes(s_f);
        let c: Arc<str> = Arc::from("c");
        assert_ne!(h_e[&c].extract_hash, h_f[&c].extract_hash);
    }
}
