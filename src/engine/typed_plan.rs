//! Read-only, storage-independent rule metadata for the internal deltaflow arc.
//! Compiling this plan has no effect on runtime execution.

use crate::ast::{BodyItem, BrandDecl, Item, MergeFn, Program, Rule, Type};
use crate::engine::type_arena::{
    StorageClass, TypeArena, TypeArenaBuilder, TypeArenaError, TypeId, TypeNode, TypedColumn,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RelId(pub(crate) [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Conservative content identity until the surface language has a stable rule
/// slot. Any executable edit therefore changes this ID and remains a runtime
/// fallback boundary.
pub(crate) struct RuleId(pub(crate) [u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Fingerprint(pub(crate) [u8; 16]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SchemaFingerprint(pub(crate) [u8; 16]);

fn stable_id(domain: &[u8], canonical: &[u8]) -> [u8; 16] {
    let mut hash = blake3::Hasher::new();
    hash.update(domain);
    hash.update(&(canonical.len() as u64).to_le_bytes());
    hash.update(canonical);
    let mut id = [0; 16];
    id.copy_from_slice(&hash.finalize().as_bytes()[..16]);
    id
}

impl RelId {
    pub(crate) fn from_name(name: &str) -> Self {
        Self(stable_id(b"sprefa.typed-plan.rel.v1\0", name.as_bytes()))
    }
}

impl RuleId {
    pub(crate) fn from_rule(rule: &Rule) -> Self {
        Self(stable_id(
            b"sprefa.typed-plan.rule-id.v1\0",
            &rule_canonical_preimage(rule),
        ))
    }
}

impl Fingerprint {
    fn from_preimage(preimage: &[u8]) -> Self {
        Self(stable_id(b"sprefa.typed-plan.fingerprint.v1\0", preimage))
    }
}

/// Versioned wrapper around the current executable structural encoding. The
/// inner `structural_key` is still Debug-derived; replacing it with a complete
/// field-by-field AST codec is deferred to the type-IR slice. The wrapper makes
/// that dependency explicit and requires a version bump if the encoding moves.
fn rule_canonical_preimage(rule: &Rule) -> Vec<u8> {
    let structural = rule.structural_key();
    let mut out = Vec::with_capacity(40 + structural.len());
    out.extend_from_slice(b"sprefa.typed-plan.rule-canonical\0");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(structural.len() as u64).to_le_bytes());
    out.extend_from_slice(structural.as_bytes());
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputKind {
    Positive,
    Negative,
    Operator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InputUse {
    pub(crate) relation: RelId,
    pub(crate) body_index: usize,
    pub(crate) occurrence: usize,
    /// Ordinal among every positive body atom, exactly matching
    /// `lower::body_sql_ex`'s occurrence counter. Non-positive inputs are None.
    pub(crate) positive_ordinal: Option<usize>,
    pub(crate) kind: InputKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperatorClass {
    PositiveAcyclic,
    PositiveRecursive,
    Negation,
    Aggregate,
    Lattice,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypedRelation {
    pub(crate) id: RelId,
    pub(crate) name: String,
    pub(crate) declared: bool,
    pub(crate) arity: Option<usize>,
    pub(crate) columns: Vec<TypedColumn>,
    pub(crate) schema_fingerprint: SchemaFingerprint,
}

// This slice intentionally compiles authored Program declarations only.
// Built-in declarations and the post-typecheck `Rels` map remain deferred so
// this read-only plan does not yet couple to runtime schema construction.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypedRule {
    pub(crate) id: RuleId,
    pub(crate) fingerprint: Fingerprint,
    canonical_preimage: Box<[u8]>,
    pub(crate) head: RelId,
    pub(crate) inputs: Vec<InputUse>,
    pub(crate) operator: OperatorClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Component {
    pub(crate) relations: Vec<RelId>,
    pub(crate) rules: Vec<RuleId>,
    /// Earlier component indices this component reads.
    pub(crate) dependencies: Vec<usize>,
    pub(crate) recursive: bool,
    pub(crate) incremental_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompileError {
    RuleIdCollision {
        id: RuleId,
    },
    DuplicateBrand {
        name: String,
    },
    UnknownBrandParent {
        brand: String,
        parent: String,
    },
    BrandCycle {
        brand: String,
    },
    UnknownColumnBrand {
        relation: String,
        column: String,
        brand: String,
    },
    TypeArena(TypeArenaError),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TypedPlan {
    pub(crate) type_arena: TypeArena,
    pub(crate) schema_fingerprint: SchemaFingerprint,
    pub(crate) relations: Vec<TypedRelation>,
    pub(crate) rules: Vec<TypedRule>,
    pub(crate) readers_of: BTreeMap<RelId, Vec<RuleId>>,
    pub(crate) writers_of: BTreeMap<RelId, Vec<RuleId>>,
    pub(crate) components: Vec<Component>,
    pub(crate) component_by_relation: BTreeMap<RelId, usize>,
}

fn input_relation(item: &BodyItem) -> Option<(&str, InputKind)> {
    match item {
        BodyItem::Pos(atom) => Some((&atom.rel, InputKind::Positive)),
        BodyItem::Neg(atom) => Some((&atom.rel, InputKind::Negative)),
        BodyItem::Closure { rel } | BodyItem::Scc { rel } | BodyItem::Node2vec { rel } => {
            Some((rel, InputKind::Operator))
        }
        _ => None,
    }
}

fn is_plain_positive(rule: &Rule) -> bool {
    rule.temporal.is_none()
        && rule
            .body
            .iter()
            .any(|item| matches!(item, BodyItem::Pos(_)))
        && rule
            .body
            .iter()
            .all(|item| matches!(item, BodyItem::Pos(_) | BodyItem::Cmp(_)))
}

fn has_external_shape(rule: &Rule) -> bool {
    rule.temporal.is_some()
        || rule
            .body
            .iter()
            .any(|item| !matches!(item, BodyItem::Pos(_) | BodyItem::Neg(_) | BodyItem::Cmp(_)))
}

fn recursive_relations(rules: &[&Rule]) -> HashSet<String> {
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for rule in rules {
        for item in &rule.body {
            if let Some((input, _)) = input_relation(item) {
                edges.entry(&rule.head.rel).or_default().push(input);
            }
        }
    }
    let mut recursive = HashSet::new();
    for start in edges.keys().copied() {
        let mut pending = edges.get(start).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        while let Some(next) = pending.pop() {
            if next == start {
                recursive.insert(start.to_string());
                break;
            }
            if seen.insert(next) {
                pending.extend(edges.get(next).into_iter().flatten().copied());
            }
        }
    }
    recursive
}

fn compile_types(
    program: &Program,
) -> Result<(TypeArenaBuilder, BTreeMap<String, TypeId>), CompileError> {
    let mut builder = TypeArenaBuilder::default();
    let mut bases = BTreeMap::new();
    for ty in [
        Type::Text,
        Type::Int,
        Type::Path,
        Type::File,
        Type::Dir,
        Type::Repo,
        Type::Rev,
    ] {
        bases.insert(
            ty.name().to_string(),
            builder
                .intern(TypeNode::Base(ty))
                .map_err(CompileError::TypeArena)?,
        );
    }
    let mut declarations = BTreeMap::<String, BrandDecl>::new();
    for item in &program.items {
        if let Item::Brand(brand) = item {
            if declarations
                .insert(brand.name.clone(), brand.clone())
                .is_some()
            {
                return Err(CompileError::DuplicateBrand {
                    name: brand.name.clone(),
                });
            }
        }
    }
    for brand in declarations.values() {
        if Type::parse(&brand.parent).is_none() && !declarations.contains_key(&brand.parent) {
            return Err(CompileError::UnknownBrandParent {
                brand: brand.name.clone(),
                parent: brand.parent.clone(),
            });
        }
    }
    let mut resolved = BTreeMap::<String, TypeId>::new();
    let mut pending = declarations;
    while !pending.is_empty() {
        let names: Vec<String> = pending.keys().cloned().collect();
        let mut progressed = false;
        for name in names {
            let declaration = &pending[&name];
            let parent = Type::parse(&declaration.parent)
                .and_then(|ty| bases.get(ty.name()).copied())
                .or_else(|| resolved.get(&declaration.parent).copied());
            let Some(parent) = parent else { continue };
            let node = match &declaration.variants {
                Some(variants) => TypeNode::Enum {
                    name: name.clone(),
                    variants: variants.clone(),
                },
                None => TypeNode::Named {
                    name: name.clone(),
                    parent,
                },
            };
            let id = builder.intern(node).map_err(CompileError::TypeArena)?;
            resolved.insert(name.clone(), id);
            pending.remove(&name);
            progressed = true;
        }
        if !progressed {
            return Err(CompileError::BrandCycle {
                brand: pending.keys().next().unwrap().clone(),
            });
        }
    }
    Ok((builder, resolved))
}

fn put_schema_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn storage_tag(storage: StorageClass) -> u8 {
    match storage {
        StorageClass::I64 => 1,
        StorageClass::InternedText => 2,
        StorageClass::RawText => 3,
        StorageClass::Blob => 4,
    }
}

fn relation_schema_fingerprint(
    name: &str,
    key: Option<&[String]>,
    merge: Option<&MergeFn>,
    columns: &[TypedColumn],
) -> SchemaFingerprint {
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"sprefa.typed-plan.relation-schema\0");
    canonical.extend_from_slice(&1u32.to_le_bytes());
    put_schema_str(&mut canonical, name);
    match key {
        None => canonical.push(0),
        Some(key) => {
            canonical.push(1);
            canonical.extend_from_slice(&(key.len() as u64).to_le_bytes());
            for column in key {
                put_schema_str(&mut canonical, column);
            }
        }
    }
    match merge {
        None => canonical.push(0),
        Some(MergeFn::MaxBy(column)) => {
            canonical.push(1);
            put_schema_str(&mut canonical, column);
        }
        Some(MergeFn::MinBy(column)) => {
            canonical.push(2);
            put_schema_str(&mut canonical, column);
        }
    }
    canonical.extend_from_slice(&(columns.len() as u64).to_le_bytes());
    for column in columns {
        put_schema_str(&mut canonical, &column.name);
        canonical.extend_from_slice(&column.logical.0);
        canonical.push(storage_tag(column.storage));
    }
    SchemaFingerprint(stable_id(
        b"sprefa.typed-plan.relation-schema-id.v1\0",
        &canonical,
    ))
}

fn plan_schema_fingerprint(relations: &[TypedRelation]) -> SchemaFingerprint {
    let mut declared: Vec<&TypedRelation> = relations
        .iter()
        .filter(|relation| relation.declared)
        .collect();
    declared.sort_by_key(|relation| relation.id);
    let mut canonical = Vec::new();
    canonical.extend_from_slice(b"sprefa.typed-plan.schema\0");
    canonical.extend_from_slice(&1u32.to_le_bytes());
    canonical.extend_from_slice(&(declared.len() as u64).to_le_bytes());
    for relation in declared {
        canonical.extend_from_slice(&relation.id.0);
        canonical.extend_from_slice(&relation.schema_fingerprint.0);
    }
    SchemaFingerprint(stable_id(b"sprefa.typed-plan.schema-id.v1\0", &canonical))
}

fn compile_components(
    relations: &[TypedRelation],
    rules: &[TypedRule],
) -> (Vec<Component>, BTreeMap<RelId, usize>) {
    let index: BTreeMap<RelId, usize> = relations
        .iter()
        .enumerate()
        .map(|(i, relation)| (relation.id, i))
        .collect();
    // Evaluation edges point input -> head, so topological order is directly
    // usable by a future delta dispatcher.
    let mut adjacency = vec![Vec::<u32>::new(); relations.len()];
    let mut self_edge = vec![false; relations.len()];
    for rule in rules {
        let head = index[&rule.head];
        for input in &rule.inputs {
            let source = index[&input.relation];
            adjacency[source].push(head as u32);
            if source == head {
                self_edge[source] = true;
            }
        }
    }
    for edges in &mut adjacency {
        edges.sort();
        edges.dedup();
    }
    let (raw_component, component_count) = crate::scc::tarjan(&adjacency);
    let mut members = vec![Vec::<RelId>::new(); component_count];
    for (relation_index, &component) in raw_component.iter().enumerate() {
        members[component as usize].push(relations[relation_index].id);
    }
    for group in &mut members {
        group.sort();
    }
    let mut outgoing = vec![BTreeSet::<usize>::new(); component_count];
    let mut incoming = vec![BTreeSet::<usize>::new(); component_count];
    for (source, edges) in adjacency.iter().enumerate() {
        let from = raw_component[source] as usize;
        for &head in edges {
            let to = raw_component[head as usize] as usize;
            if from != to {
                outgoing[from].insert(to);
                incoming[to].insert(from);
            }
        }
    }
    let mut ready = BTreeSet::<(RelId, usize)>::new();
    let mut indegree: Vec<usize> = incoming.iter().map(BTreeSet::len).collect();
    for component in 0..component_count {
        if indegree[component] == 0 {
            ready.insert((members[component][0], component));
        }
    }
    let mut order = Vec::with_capacity(component_count);
    while let Some(&(key, component)) = ready.iter().next() {
        ready.remove(&(key, component));
        order.push(component);
        for &next in &outgoing[component] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.insert((members[next][0], next));
            }
        }
    }
    debug_assert_eq!(order.len(), component_count);
    let remap: HashMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();
    let mut components = Vec::with_capacity(component_count);
    let mut component_by_relation = BTreeMap::new();
    for (new_index, &old_index) in order.iter().enumerate() {
        let mut component_rules: Vec<RuleId> = rules
            .iter()
            .filter(|rule| raw_component[index[&rule.head]] as usize == old_index)
            .map(|rule| rule.id)
            .collect();
        component_rules.sort();
        let recursive = members[old_index].len() > 1
            || members[old_index]
                .iter()
                .any(|relation| self_edge[index[relation]]);
        let incremental_eligible = !component_rules.is_empty()
            && rules
                .iter()
                .filter(|rule| raw_component[index[&rule.head]] as usize == old_index)
                .all(|rule| rule.operator == OperatorClass::PositiveAcyclic);
        let mut dependencies: Vec<usize> = incoming[old_index]
            .iter()
            .map(|dependency| remap[dependency])
            .collect();
        dependencies.sort();
        for &relation in &members[old_index] {
            component_by_relation.insert(relation, new_index);
        }
        components.push(Component {
            relations: members[old_index].clone(),
            rules: component_rules,
            dependencies,
            recursive,
            incremental_eligible,
        });
    }
    (components, component_by_relation)
}

impl TypedPlan {
    pub(crate) fn compile(program: &Program) -> Result<Self, CompileError> {
        Self::compile_with_rule_hasher(program, |preimage| {
            RuleId(stable_id(b"sprefa.typed-plan.rule-id.v1\0", preimage))
        })
    }

    fn compile_with_rule_hasher<F>(program: &Program, rule_id: F) -> Result<Self, CompileError>
    where
        F: Fn(&[u8]) -> RuleId,
    {
        let (mut type_builder, brands) = compile_types(program)?;
        let source_rules: Vec<&Rule> = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rule(rule) => Some(rule),
                _ => None,
            })
            .collect();
        let recursive = recursive_relations(&source_rules);
        let mut declared = BTreeMap::new();
        let mut relation_names = BTreeSet::new();
        let mut lattice_heads = HashSet::new();
        for item in &program.items {
            if let Item::Rel(rel) = item {
                relation_names.insert(rel.name.clone());
                declared.insert(rel.name.clone(), rel);
                // Any narrowed key changes row identity away from full-row set
                // semantics. With or without an explicit merge, it cannot use
                // the first positive Boolean-delta implementation safely.
                if rel.key.is_some() || rel.merge.is_some() {
                    lattice_heads.insert(rel.name.clone());
                }
            }
        }
        for rule in &source_rules {
            relation_names.insert(rule.head.rel.clone());
            for item in &rule.body {
                if let Some((name, _)) = input_relation(item) {
                    relation_names.insert(name.to_string());
                }
            }
        }

        let mut relation_by_name = HashMap::new();
        let mut relations = Vec::new();
        for name in relation_names {
            let id = RelId::from_name(&name);
            relation_by_name.insert(name.clone(), id);
            let declaration = declared.get(&name).copied();
            let mut columns = Vec::new();
            if let Some(relation) = declaration {
                for column in &relation.cols {
                    if let Some(brand) = column.brand.as_deref() {
                        if !brands.contains_key(brand) {
                            return Err(CompileError::UnknownColumnBrand {
                                relation: name.clone(),
                                column: column.name.clone(),
                                brand: brand.to_string(),
                            });
                        }
                    }
                    columns.push(
                        type_builder
                            .lower_col(column, &brands)
                            .map_err(CompileError::TypeArena)?,
                    );
                }
            }
            let schema_fingerprint = relation_schema_fingerprint(
                &name,
                declaration.and_then(|relation| relation.key.as_deref()),
                declaration.and_then(|relation| relation.merge.as_ref()),
                &columns,
            );
            relations.push(TypedRelation {
                id,
                name,
                declared: declaration.is_some(),
                arity: declaration.map(|relation| relation.cols.len()),
                columns,
                schema_fingerprint,
            });
        }
        relations.sort_by_key(|relation| relation.id);
        let schema_fingerprint = plan_schema_fingerprint(&relations);
        let type_arena = type_builder.finish();

        let mut unique_rules = BTreeMap::<RuleId, TypedRule>::new();
        for rule in source_rules {
            let canonical_preimage = rule_canonical_preimage(rule);
            let id = rule_id(&canonical_preimage);
            if let Some(existing) = unique_rules.get(&id) {
                if existing.canonical_preimage.as_ref() != canonical_preimage.as_slice() {
                    return Err(CompileError::RuleIdCollision { id });
                }
                continue;
            }
            let fingerprint = Fingerprint::from_preimage(&canonical_preimage);
            let mut occurrences = HashMap::<RelId, usize>::new();
            let mut positive_ordinal = 0usize;
            let inputs = rule
                .body
                .iter()
                .enumerate()
                .filter_map(|(body_index, item)| {
                    let (name, kind) = input_relation(item)?;
                    let relation = relation_by_name[name];
                    let occurrence = *occurrences
                        .entry(relation)
                        .and_modify(|n| *n += 1)
                        .or_insert(0);
                    let ordinal = if kind == InputKind::Positive {
                        let ordinal = positive_ordinal;
                        positive_ordinal += 1;
                        Some(ordinal)
                    } else {
                        None
                    };
                    Some(InputUse {
                        relation,
                        body_index,
                        occurrence,
                        positive_ordinal: ordinal,
                        kind,
                    })
                })
                .collect();
            let plain = is_plain_positive(rule);
            let operator = if has_external_shape(rule) {
                OperatorClass::External
            } else if rule.has_agg() {
                OperatorClass::Aggregate
            } else if rule
                .body
                .iter()
                .any(|item| matches!(item, BodyItem::Neg(_)))
            {
                OperatorClass::Negation
            } else if !plain {
                OperatorClass::External
            } else if lattice_heads.contains(&rule.head.rel) {
                OperatorClass::Lattice
            } else if recursive.contains(&rule.head.rel) {
                OperatorClass::PositiveRecursive
            } else {
                OperatorClass::PositiveAcyclic
            };
            unique_rules.insert(
                id,
                TypedRule {
                    id,
                    fingerprint,
                    canonical_preimage: canonical_preimage.into_boxed_slice(),
                    head: relation_by_name[&rule.head.rel],
                    inputs,
                    operator,
                },
            );
        }
        let rules: Vec<TypedRule> = unique_rules.into_values().collect();
        let mut readers_of = BTreeMap::<RelId, Vec<RuleId>>::new();
        let mut writers_of = BTreeMap::<RelId, Vec<RuleId>>::new();
        for rule in &rules {
            writers_of.entry(rule.head).or_default().push(rule.id);
            for input in &rule.inputs {
                readers_of.entry(input.relation).or_default().push(rule.id);
            }
        }
        for ids in readers_of.values_mut() {
            ids.sort();
            ids.dedup();
        }
        for ids in writers_of.values_mut() {
            ids.sort();
            ids.dedup();
        }
        let (components, component_by_relation) = compile_components(&relations, &rules);
        Ok(Self {
            type_arena,
            schema_fingerprint,
            relations,
            rules,
            readers_of,
            writers_of,
            components,
            component_by_relation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AggFn, Atom, BodyItem, Col, MergeFn, RelDecl, Temporal, Term, Type};
    use std::path::PathBuf;

    fn atom(rel: &str) -> Atom {
        Atom {
            rel: rel.into(),
            terms: vec![Term::Var("X".into())],
            named: Vec::new(),
        }
    }

    fn rule(head: &str, body: Vec<BodyItem>) -> Rule {
        Rule {
            head: atom(head),
            body,
            aggs: Vec::new(),
            agg_args2: Vec::new(),
            origin: None,
            temporal: None,
        }
    }

    fn rel(name: &str) -> RelDecl {
        RelDecl {
            name: name.into(),
            cols: vec![Col::plain("x".into(), Type::Text)],
            ..Default::default()
        }
    }

    fn class(plan: &TypedPlan, head: &str) -> OperatorClass {
        let id = RelId::from_name(head);
        plan.rules
            .iter()
            .find(|rule| rule.head == id)
            .unwrap()
            .operator
    }

    fn component<'a>(plan: &'a TypedPlan, relation: &str) -> &'a Component {
        &plan.components[plan.component_by_relation[&RelId::from_name(relation)]]
    }

    fn typed_relation<'a>(plan: &'a TypedPlan, name: &str) -> &'a TypedRelation {
        plan.relations
            .iter()
            .find(|relation| relation.name == name)
            .unwrap()
    }

    #[test]
    fn ids_and_plan_order_ignore_origin_and_item_order() {
        let mut first = rule("out", vec![BodyItem::Pos(atom("input"))]);
        first.origin = Some(PathBuf::from("one.dl"));
        let mut second = first.clone();
        second.origin = Some(PathBuf::from("elsewhere/two.dl"));
        assert_eq!(RuleId::from_rule(&first), RuleId::from_rule(&second));
        let a = TypedPlan::compile(&Program {
            items: vec![Item::Rel(rel("input")), Item::Rule(first)],
        })
        .unwrap();
        let b = TypedPlan::compile(&Program {
            items: vec![Item::Rule(second), Item::Rel(rel("input"))],
        })
        .unwrap();
        assert_eq!(a, b);
        assert_ne!(RelId::from_name("Input"), RelId::from_name("input"));
    }

    #[test]
    fn canonical_rule_id_and_fingerprint_have_golden_encodings() {
        let executable = rule("out", vec![BodyItem::Pos(atom("input"))]);
        assert_eq!(
            RuleId::from_rule(&executable).0,
            [42, 59, 188, 53, 102, 89, 3, 33, 109, 214, 161, 67, 95, 134, 171, 56]
        );
        assert_eq!(
            Fingerprint::from_preimage(&rule_canonical_preimage(&executable)).0,
            [39, 200, 92, 110, 89, 77, 161, 150, 16, 86, 177, 30, 79, 101, 192, 77]
        );
    }

    #[test]
    fn unequal_canonical_rules_cannot_silently_share_an_injected_id() {
        let program = Program {
            items: vec![
                Item::Rule(rule("a", vec![BodyItem::Pos(atom("input"))])),
                Item::Rule(rule("b", vec![BodyItem::Pos(atom("input"))])),
            ],
        };
        let collision = TypedPlan::compile_with_rule_hasher(&program, |_| RuleId([7; 16]));
        assert_eq!(
            collision,
            Err(CompileError::RuleIdCollision {
                id: RuleId([7; 16])
            })
        );
    }

    #[test]
    fn records_occurrences_and_reader_writer_adjacency() {
        let plan = TypedPlan::compile(&Program {
            items: vec![Item::Rule(rule(
                "out",
                vec![
                    BodyItem::Pos(atom("a")),
                    BodyItem::Pos(atom("b")),
                    BodyItem::Pos(atom("a")),
                ],
            ))],
        })
        .unwrap();
        let typed = &plan.rules[0];
        assert_eq!(
            typed
                .inputs
                .iter()
                .map(|input| input.positive_ordinal)
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1), Some(2)]
        );
        assert_eq!(
            typed
                .inputs
                .iter()
                .map(|input| input.occurrence)
                .collect::<Vec<_>>(),
            vec![0, 0, 1]
        );
        assert_eq!(plan.readers_of[&RelId::from_name("a")], vec![typed.id]);
        assert_eq!(plan.readers_of[&RelId::from_name("b")], vec![typed.id]);
        assert_eq!(plan.writers_of[&RelId::from_name("out")], vec![typed.id]);
    }

    #[test]
    fn positive_cycles_are_recursive_but_plain_chains_are_acyclic() {
        let plan = TypedPlan::compile(&Program {
            items: vec![
                Item::Rule(rule("a", vec![BodyItem::Pos(atom("b"))])),
                Item::Rule(rule("b", vec![BodyItem::Pos(atom("a"))])),
                Item::Rule(rule("sink", vec![BodyItem::Pos(atom("a"))])),
            ],
        })
        .unwrap();
        assert_eq!(class(&plan, "a"), OperatorClass::PositiveRecursive);
        assert_eq!(class(&plan, "b"), OperatorClass::PositiveRecursive);
        assert_eq!(class(&plan, "sink"), OperatorClass::PositiveAcyclic);
        let cycle = component(&plan, "a");
        assert_eq!(cycle, component(&plan, "b"));
        assert!(cycle.recursive);
        assert!(!cycle.incremental_eligible);
        let sink = component(&plan, "sink");
        assert!(!sink.recursive);
        assert!(sink.incremental_eligible);
        assert!(sink
            .dependencies
            .contains(&plan.component_by_relation[&RelId::from_name("a")]));
    }

    #[test]
    fn unsupported_operator_classes_never_claim_positive_acyclic() {
        let mut aggregate = rule("aggregate", vec![BodyItem::Pos(atom("input"))]);
        aggregate.aggs = vec![Some(AggFn::Count)];
        let negative = rule("negative", vec![BodyItem::Neg(atom("input"))]);
        let external = rule(
            "external",
            vec![BodyItem::Closure {
                rel: "input".into(),
            }],
        );
        let mut lattice_rel = rel("lattice");
        lattice_rel.key = Some(vec!["x".into()]);
        lattice_rel.merge = Some(MergeFn::MaxBy("x".into()));
        let lattice = rule("lattice", vec![BodyItem::Pos(atom("input"))]);
        let plan = TypedPlan::compile(&Program {
            items: vec![
                Item::Rel(lattice_rel),
                Item::Rule(aggregate),
                Item::Rule(negative),
                Item::Rule(external),
                Item::Rule(lattice),
            ],
        })
        .unwrap();
        assert_eq!(class(&plan, "aggregate"), OperatorClass::Aggregate);
        assert_eq!(class(&plan, "negative"), OperatorClass::Negation);
        assert_eq!(class(&plan, "external"), OperatorClass::External);
        assert_eq!(class(&plan, "lattice"), OperatorClass::Lattice);

        let mut choice_rel = rel("choice");
        choice_rel.key = Some(vec!["x".into()]);
        let choice = rule("choice", vec![BodyItem::Pos(atom("input"))]);
        let choice_plan = TypedPlan::compile(&Program {
            items: vec![Item::Rel(choice_rel), Item::Rule(choice)],
        })
        .unwrap();
        assert_eq!(class(&choice_plan, "choice"), OperatorClass::Lattice);
    }

    #[test]
    fn mixed_same_head_component_and_temporal_rule_fall_back() {
        let positive = rule("mixed", vec![BodyItem::Pos(atom("input"))]);
        let operator = rule("mixed", vec![BodyItem::Closure { rel: "edge".into() }]);
        let mut temporal = rule("temporal", vec![BodyItem::Pos(atom("temporal"))]);
        temporal.temporal = Some(Temporal::Next);
        temporal.aggs = vec![Some(AggFn::Count)];
        let plan = TypedPlan::compile(&Program {
            items: vec![
                Item::Rule(positive),
                Item::Rule(operator),
                Item::Rule(temporal),
            ],
        })
        .unwrap();
        let mixed = component(&plan, "mixed");
        assert_eq!(mixed.rules.len(), 2);
        assert!(!mixed.incremental_eligible);
        assert_eq!(class(&plan, "temporal"), OperatorClass::External);
        assert!(component(&plan, "temporal").recursive);
        assert!(!component(&plan, "temporal").incremental_eligible);
    }

    #[test]
    fn brand_chain_and_enum_lower_to_addressed_nodes() {
        let sha = BrandDecl {
            name: "sha".into(),
            parent: "text".into(),
            variants: None,
        };
        let commit = BrandDecl {
            name: "commit".into(),
            parent: "sha".into(),
            variants: None,
        };
        let severity = BrandDecl {
            name: "severity".into(),
            parent: "text".into(),
            variants: Some(vec!["warn".into(), "error".into()]),
        };
        let mut declaration = rel("typed");
        declaration.cols = vec![
            Col::branded("commit", "commit"),
            Col::branded("severity", "severity"),
        ];
        let plan = TypedPlan::compile(&Program {
            items: vec![
                Item::Brand(commit),
                Item::Brand(severity),
                Item::Rel(declaration),
                Item::Brand(sha),
            ],
        })
        .unwrap();
        let columns = &typed_relation(&plan, "typed").columns;
        let TypeNode::Named { name, parent } = plan.type_arena.get(columns[0].logical).unwrap()
        else {
            panic!("commit is not nominal")
        };
        assert_eq!(name, "commit");
        assert!(
            matches!(plan.type_arena.get(*parent), Some(TypeNode::Named { name, .. }) if name == "sha")
        );
        assert_eq!(
            plan.type_arena.get(columns[1].logical),
            Some(&TypeNode::Enum {
                name: "severity".into(),
                variants: vec!["warn".into(), "error".into()],
            })
        );
    }

    #[test]
    fn typed_columns_preserve_raw_interned_and_integer_storage() {
        let mut declaration = rel("storage");
        declaration.cols = vec![
            Col::plain("interned".into(), Type::Text),
            Col::raw("raw", Type::Text),
            Col::plain("number".into(), Type::Int),
        ];
        let plan = TypedPlan::compile(&Program {
            items: vec![Item::Rel(declaration)],
        })
        .unwrap();
        let columns = &typed_relation(&plan, "storage").columns;
        assert_eq!(
            columns
                .iter()
                .map(|column| column.storage)
                .collect::<Vec<_>>(),
            vec![
                StorageClass::InternedText,
                StorageClass::RawText,
                StorageClass::I64,
            ]
        );
        assert!(matches!(
            plan.type_arena.get(columns[0].logical),
            Some(TypeNode::Base(Type::Text))
        ));
        assert!(matches!(
            plan.type_arena.get(columns[2].logical),
            Some(TypeNode::Base(Type::Int))
        ));
    }

    #[test]
    fn schema_edit_changes_fingerprint_not_relation_identity() {
        let text = rel("schema");
        let mut integer = rel("schema");
        integer.cols = vec![Col::plain("x".into(), Type::Int)];
        let a = TypedPlan::compile(&Program {
            items: vec![Item::Rel(text)],
        })
        .unwrap();
        let b = TypedPlan::compile(&Program {
            items: vec![Item::Rel(integer)],
        })
        .unwrap();
        assert_eq!(
            typed_relation(&a, "schema").id,
            typed_relation(&b, "schema").id
        );
        assert_ne!(
            typed_relation(&a, "schema").schema_fingerprint,
            typed_relation(&b, "schema").schema_fingerprint
        );
        assert_ne!(a.schema_fingerprint, b.schema_fingerprint);
    }

    #[test]
    fn rule_edits_do_not_move_schema_fingerprint() {
        let a = TypedPlan::compile(&Program {
            items: vec![
                Item::Rel(rel("schema")),
                Item::Rule(rule("out", vec![BodyItem::Pos(atom("left"))])),
            ],
        })
        .unwrap();
        let b = TypedPlan::compile(&Program {
            items: vec![
                Item::Rule(rule("out", vec![BodyItem::Pos(atom("right"))])),
                Item::Rel(rel("schema")),
            ],
        })
        .unwrap();
        assert_eq!(a.schema_fingerprint, b.schema_fingerprint);
    }

    #[test]
    fn unknown_brand_parent_and_column_brand_are_refused() {
        let unknown_parent = TypedPlan::compile(&Program {
            items: vec![Item::Brand(BrandDecl {
                name: "child".into(),
                parent: "missing".into(),
                variants: None,
            })],
        });
        assert_eq!(
            unknown_parent,
            Err(CompileError::UnknownBrandParent {
                brand: "child".into(),
                parent: "missing".into(),
            })
        );
        let cycle = TypedPlan::compile(&Program {
            items: vec![
                Item::Brand(BrandDecl {
                    name: "a".into(),
                    parent: "b".into(),
                    variants: None,
                }),
                Item::Brand(BrandDecl {
                    name: "b".into(),
                    parent: "a".into(),
                    variants: None,
                }),
            ],
        });
        assert_eq!(cycle, Err(CompileError::BrandCycle { brand: "a".into() }));
        let mut declaration = rel("bad_column");
        declaration.cols = vec![Col::branded("x", "missing")];
        assert_eq!(
            TypedPlan::compile(&Program {
                items: vec![Item::Rel(declaration)]
            }),
            Err(CompileError::UnknownColumnBrand {
                relation: "bad_column".into(),
                column: "x".into(),
                brand: "missing".into(),
            })
        );
    }

    #[test]
    fn brand_and_relation_item_order_is_deterministic() {
        let parent = BrandDecl {
            name: "parent".into(),
            parent: "text".into(),
            variants: None,
        };
        let child = BrandDecl {
            name: "child".into(),
            parent: "parent".into(),
            variants: None,
        };
        let mut declaration = rel("ordered");
        declaration.cols = vec![Col::branded("value", "child")];
        let a = TypedPlan::compile(&Program {
            items: vec![
                Item::Brand(parent.clone()),
                Item::Brand(child.clone()),
                Item::Rel(declaration.clone()),
            ],
        })
        .unwrap();
        let b = TypedPlan::compile(&Program {
            items: vec![
                Item::Rel(declaration),
                Item::Brand(child),
                Item::Brand(parent),
            ],
        })
        .unwrap();
        assert_eq!(a, b);
    }
}
