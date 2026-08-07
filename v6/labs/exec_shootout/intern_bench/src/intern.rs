use rustc_hash::FxHashMap;

// The intern table the plan-IR contract projects: one FxHashMap from string to
// id plus a Vec reverse table, sized for the four TEXT columns of a v6 rel.
pub struct Interner {
    ids: FxHashMap<Box<str>, u32>,
    reverse: Vec<Box<str>>,
}

impl Default for Interner {
    fn default() -> Self {
        Interner {
            ids: FxHashMap::default(),
            reverse: Vec::new(),
        }
    }
}

impl Interner {
    pub fn intern(&mut self, text: &str) -> u32 {
        if let Some(existing) = self.ids.get(text) {
            return *existing;
        }
        let id = self.reverse.len() as u32;
        let owned: Box<str> = Box::from(text);
        self.reverse.push(owned.clone());
        self.ids.insert(owned, id);
        id
    }

    pub fn text(&self, id: u32) -> &str {
        &self.reverse[id as usize]
    }

    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }
}

// A (path_id, name_id) pair is one graph node; collapsing the pair to a single
// u32 is what lets the fixpoint keep interp's two-column tuple shape.
#[derive(Default)]
pub struct NodeTable {
    ids: FxHashMap<(u32, u32), u32>,
    reverse: Vec<(u32, u32)>,
}

impl NodeTable {
    pub fn node_for(&mut self, path_id: u32, name_id: u32) -> u32 {
        if let Some(existing) = self.ids.get(&(path_id, name_id)) {
            return *existing;
        }
        let id = self.reverse.len() as u32;
        self.reverse.push((path_id, name_id));
        self.ids.insert((path_id, name_id), id);
        id
    }

    pub fn columns(&self, node: u32) -> (u32, u32) {
        self.reverse[node as usize]
    }

    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_text_gets_the_same_id_and_no_new_row() {
        let mut interner = Interner::default();
        let first = interner.intern("src/engine/lower/pass_0/module_0.ts");
        let second = interner.intern("src/engine/lower/pass_0/module_0.ts");
        assert_eq!(first, second);
        assert_eq!(interner.len(), 1);
        assert_eq!(interner.text(first), "src/engine/lower/pass_0/module_0.ts");
    }

    #[test]
    fn node_table_collapses_a_pair_and_hands_it_back() {
        let mut nodes = NodeTable::default();
        let node = nodes.node_for(3, 7);
        assert_eq!(nodes.node_for(3, 7), node);
        assert_eq!(nodes.columns(node), (3, 7));
        assert_eq!(nodes.len(), 1);
    }
}
