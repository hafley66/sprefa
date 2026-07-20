use crate::eval::enumerate_paths;
use crate::facts::{Fact, FactStore};
use crate::store::{Declaration, Store};

pub fn saturate(store: &Store, facts: &mut FactStore) -> usize {
    let mut inserted = 0;
    for (id, ty) in store.types.iter().enumerate() {
        if facts.insert(Fact::TypeKind(
            crate::TypeId(id as u32),
            ty.kind_name().to_owned(),
        )) {
            inserted += 1;
        }
    }
    for declaration in store.declarations.values() {
        if let Declaration::Pattern(pattern) = declaration {
            for slot in crate::eval::enumerate_slots(store, *pattern) {
                if facts.insert(Fact::SlotType(*pattern, slot.position, slot.ty)) {
                    inserted += 1;
                }
            }
        }
    }
    for (domain, _, pattern, output) in &store.consumers {
        if facts.insert(Fact::Consumer(domain.clone(), *pattern, *output)) {
            inserted += 1;
        }
    }
    for declaration in store.declarations.values() {
        if let Declaration::Type(root) = declaration {
            for path in enumerate_paths(store, *root) {
                if facts.insert(Fact::Path(*root, path.text, path.leaf)) {
                    inserted += 1;
                }
            }
        }
    }
    inserted
}
