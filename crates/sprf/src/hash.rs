/// Hash computation for sprf_meta rule change detection.
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use sprefa_rules::graph::DepEdge;
use sprefa_rules::types::Rule;

#[derive(Debug, Clone)]
pub struct RuleHashes {
    pub schema_hash: String,
    pub extract_hash: String,
}

pub fn compute_rule_hashes(
    rules: &[Rule],
    edges: &[DepEdge],
) -> Result<HashMap<String, RuleHashes>> {
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for rule in rules {
        in_degree.insert(rule.name.as_str(), 0);
    }
    for edge in edges {
        dependents
            .entry(edge.producer.as_str())
            .or_default()
            .push(edge.consumer.as_str());
        *in_degree.entry(edge.consumer.as_str()).or_insert(0) += 1;
    }
    let mut sorted: Vec<&str> = Vec::with_capacity(rules.len());
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();
    while !queue.is_empty() {
        let node = queue.pop().unwrap();
        sorted.push(node);
        if let Some(deps) = dependents.get(node) {
            for &dep in deps {
                let deg = in_degree.get_mut(dep).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push(dep);
                }
            }
        }
    }
    let mut hashes: HashMap<String, RuleHashes> = HashMap::with_capacity(rules.len());
    let rule_map: HashMap<&str, &Rule> = rules.iter().map(|r| (r.name.as_str(), r)).collect();
    for rule_name in sorted {
        let rule = rule_map.get(rule_name).unwrap();
        let schema_hash = compute_schema_hash(rule);
        let extract_hash = compute_extract_hash(rule, &hashes, edges);
        hashes.insert(
            rule.name.clone(),
            RuleHashes {
                schema_hash,
                extract_hash,
            },
        );
    }
    Ok(hashes)
}

fn compute_schema_hash(rule: &Rule) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rule.name.as_bytes());
    for m in &rule.create_matches {
        hasher.update(m.kind.as_bytes());
        if let Some(ref scan) = m.scan {
            hasher.update(scan.as_bytes());
        }
    }
    hex_encode(&hasher.finalize())
}

fn compute_extract_hash(
    rule: &Rule,
    computed: &HashMap<String, RuleHashes>,
    edges: &[DepEdge],
) -> String {
    let mut hasher = Sha256::new();
    for step in &rule.select {
        hash_select_step(step, &mut hasher);
    }
    if let Some(ref value) = rule.value {
        hasher.update(b"value");
        match value {
            sprefa_rules::types::LineMatcher::Segments { pattern, .. } => {
                hasher.update(pattern.as_bytes())
            }
            sprefa_rules::types::LineMatcher::Regex { pattern, .. } => {
                hasher.update(pattern.as_bytes())
            }
        }
    }
    let producer_edges: Vec<&DepEdge> = edges.iter().filter(|e| e.consumer == rule.name).collect();
    for edge in producer_edges {
        if let Some(producer_hashes) = computed.get(&edge.producer) {
            hasher.update(producer_hashes.extract_hash.as_bytes());
            for (prod_col, cons_var) in &edge.bindings {
                hasher.update(prod_col.as_bytes());
                hasher.update(cons_var.as_bytes());
            }
        }
    }
    hex_encode(&hasher.finalize())
}

fn hash_select_step(step: &sprefa_rules::types::SelectStep, hasher: &mut Sha256) {
    use sprefa_rules::types::SelectStep;
    match step {
        SelectStep::Repo { pattern, .. } => {
            hasher.update(b"repo");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::Rev { pattern, .. } => {
            hasher.update(b"rev");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::Folder { pattern, .. } => {
            hasher.update(b"folder");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::File { pattern, .. } => {
            hasher.update(b"file");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::Key { name, .. } => {
            hasher.update(b"key");
            hasher.update(name.as_bytes());
        }
        SelectStep::KeyMatch { pattern, .. } => {
            hasher.update(b"keymatch");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::Any => hasher.update(b"any"),
        SelectStep::DepthMin { n } => {
            hasher.update(b"depthmin");
            hasher.update(&n.to_le_bytes());
        }
        SelectStep::DepthMax { n } => {
            hasher.update(b"depthmax");
            hasher.update(&n.to_le_bytes());
        }
        SelectStep::DepthEq { n } => {
            hasher.update(b"deptheq");
            hasher.update(&n.to_le_bytes());
        }
        SelectStep::ParentKey { pattern } => {
            hasher.update(b"parentkey");
            hasher.update(pattern.as_bytes());
        }
        SelectStep::ArrayItem => hasher.update(b"arrayitem"),
        SelectStep::Leaf { .. } => hasher.update(b"leaf"),
        SelectStep::Object { entries } => {
            hasher.update(b"object");
            for e in entries {
                match &e.key {
                    sprefa_rules::types::KeyMatcher::Exact(s) => {
                        hasher.update(b"exact");
                        hasher.update(s.as_bytes());
                    }
                    sprefa_rules::types::KeyMatcher::Glob(s) => {
                        hasher.update(b"glob");
                        hasher.update(s.as_bytes());
                    }
                    sprefa_rules::types::KeyMatcher::Capture(s) => {
                        hasher.update(b"capture");
                        hasher.update(s.as_bytes());
                    }
                    sprefa_rules::types::KeyMatcher::Wildcard => hasher.update(b"wildcard"),
                }
            }
        }
        SelectStep::Array { item } => {
            hasher.update(b"array");
            for s in item {
                hash_select_step(s, hasher);
            }
        }
        SelectStep::LeafPattern { pattern } => {
            hasher.update(b"leafpattern");
            hasher.update(pattern.as_bytes());
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_sprf;
    #[test]
    fn single_rule() {
        let s = r#"rule(pkg) { fs(**/p.json) > json({ name: $NAME }) };"#;
        let (rs, e) = parse_sprf(s).unwrap();
        let h = compute_rule_hashes(&rs.rules, &e).unwrap();
        assert_eq!(h.len(), 1);
    }
    #[test]
    fn schema_changes_with_column() {
        let s1 = r#"rule(p) { fs(**/p.json) > json({ name: $N }) };"#;
        let s2 = r#"rule(p) { fs(**/p.json) > json({ name: $N, ver: $V }) };"#;
        let (r1, e1) = parse_sprf(s1).unwrap();
        let (r2, e2) = parse_sprf(s2).unwrap();
        let h1 = compute_rule_hashes(&r1.rules, &e1).unwrap();
        let h2 = compute_rule_hashes(&r2.rules, &e2).unwrap();
        assert_ne!(
            h1.get("p").unwrap().schema_hash,
            h2.get("p").unwrap().schema_hash
        );
    }
}
