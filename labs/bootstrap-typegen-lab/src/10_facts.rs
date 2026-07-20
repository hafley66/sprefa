use crate::{PatternId, Symbol, TypeId};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Fact {
    TypeKind(TypeId, String),
    Field(TypeId, Symbol, TypeId),
    SlotType(PatternId, u32, TypeId),
    Consumer(String, PatternId, TypeId),
    Path(TypeId, String, TypeId),
}

#[derive(Default, Debug)]
pub struct FactStore {
    pub facts: Vec<Fact>,
}

impl FactStore {
    pub fn insert(&mut self, fact: Fact) -> bool {
        if self.facts.contains(&fact) {
            false
        } else {
            self.facts.push(fact);
            true
        }
    }
}
