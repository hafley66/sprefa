use anyhow::Result;
use protobuf::Message;
use scip::types::{Index, SymbolRole};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct ScipRows {
    pub defs: Vec<(String, String)>,
    pub refs: Vec<(String, String, String)>,
    pub edges: Vec<(String, String)>,
}

pub fn index_path(root: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SPREFA_SCIP_INDEX") {
        let path = PathBuf::from(path);
        if path.is_file() { return Some(path); }
    }
    let path = root.join("index.scip");
    path.is_file().then_some(path)
}

pub fn load(path: &Path) -> Result<ScipRows> {
    let bytes = std::fs::read(path)?;
    let index = Index::parse_from_bytes(&bytes)?;
    Ok(rows(&index))
}

pub fn rows(index: &Index) -> ScipRows {
    let mut def_file: HashMap<String, String> = HashMap::new();
    let mut defs: HashSet<(String, String)> = HashSet::new();

    for doc in &index.documents {
        for occ in &doc.occurrences {
            if !usable_symbol(&occ.symbol) { continue; }
            if is_def(occ.symbol_roles) {
                def_file.entry(occ.symbol.clone()).or_insert_with(|| doc.relative_path.clone());
                defs.insert((occ.symbol.clone(), doc.relative_path.clone()));
            }
        }
    }

    let mut refs: HashSet<(String, String, String)> = HashSet::new();
    let mut edges: HashSet<(String, String)> = HashSet::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if !usable_symbol(&occ.symbol) || is_def(occ.symbol_roles) { continue; }
            let Some(def) = def_file.get(&occ.symbol) else { continue };
            refs.insert((doc.relative_path.clone(), occ.symbol.clone(), def.clone()));
            if def != &doc.relative_path {
                edges.insert((doc.relative_path.clone(), def.clone()));
            }
        }
    }

    let mut rows = ScipRows {
        defs: defs.into_iter().collect(),
        refs: refs.into_iter().collect(),
        edges: edges.into_iter().collect(),
    };
    rows.defs.sort();
    rows.refs.sort();
    rows.edges.sort();
    rows
}

fn is_def(roles: i32) -> bool {
    roles & (SymbolRole::Definition as i32) != 0
}

fn usable_symbol(symbol: &str) -> bool {
    !symbol.is_empty() && !symbol.starts_with("local ")
}
