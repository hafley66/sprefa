use std::collections::HashSet;

use crate::patterns::*;
use crate::store::Store;
use crate::types::{Primitive, Type, Value};
use crate::{PatternId, Symbol, TypeId};

pub fn bind(
    store: &Store,
    pattern: PatternId,
    args: &[ArgumentValue],
) -> Result<String, PatternError> {
    let pattern = store
        .patterns
        .get(pattern.0 as usize)
        .ok_or(PatternError::UnknownPattern(pattern.0.to_string()))?;
    let slots = pattern
        .parts
        .iter()
        .filter_map(|part| match part {
            PatternPart::Slot(slot) => Some(slot),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut positional = args.iter().filter_map(|arg| match arg {
        ArgumentValue::Positional(value) => Some(value.clone()),
        _ => None,
    });
    let mut named = Vec::new();
    let mut used = HashSet::new();
    let mut rendered = String::new();
    for part in &pattern.parts {
        match part {
            PatternPart::Literal { text, .. } => rendered.push_str(text),
            PatternPart::Slot(slot) => {
                let value = args
                    .iter()
                    .find_map(|arg| match arg {
                        ArgumentValue::Named(name, value)
                            if slot
                                .name
                                .map(|symbol| store.symbols.resolve(symbol) == name)
                                .unwrap_or(false) =>
                        {
                            Some(value.clone())
                        }
                        _ => None,
                    })
                    .or_else(|| positional.next())
                    .ok_or_else(|| PatternError::MissingBinding(slot.source.clone()))?;
                let name = slot
                    .name
                    .map(|symbol| store.symbols.resolve(symbol).to_owned())
                    .unwrap_or_default();
                if !used.insert(name.clone()) {
                    return Err(PatternError::DuplicateBinding(name));
                }
                validate(store, slot.ty, &value)?;
                named.push(name);
                rendered.push_str(&value_text(&value));
            }
        }
    }
    if args.len() > slots.len() {
        return Err(PatternError::ExtraBinding("too many arguments".to_owned()));
    }
    Ok(rendered)
}

pub fn match_pattern(
    store: &Store,
    pattern: PatternId,
    input: &str,
) -> Result<Bindings, PatternError> {
    let pattern = store
        .patterns
        .get(pattern.0 as usize)
        .ok_or(PatternError::UnknownPattern(pattern.0.to_string()))?;
    let mut cursor = 0;
    let mut positional = Vec::new();
    let mut named = Vec::new();
    for (index, part) in pattern.parts.iter().enumerate() {
        match part {
            PatternPart::Literal { text, .. } => {
                if !input[cursor..].starts_with(text) {
                    return Err(PatternError::NoMatch);
                }
                cursor += text.len();
            }
            PatternPart::Slot(slot) => {
                let next_literal = pattern.parts[index + 1..]
                    .iter()
                    .find_map(|part| match part {
                        PatternPart::Literal { text, .. } => Some(text),
                        _ => None,
                    });
                let end = next_literal
                    .map(|literal| input[cursor..].find(literal).map(|offset| cursor + offset))
                    .flatten()
                    .unwrap_or(input.len());
                if next_literal.is_none()
                    && pattern.parts[index + 1..]
                        .iter()
                        .any(|part| matches!(part, PatternPart::Slot(_)))
                {
                    return Err(PatternError::AmbiguousMatch("adjacent slots".to_owned()));
                }
                let raw = input.get(cursor..end).ok_or(PatternError::NoMatch)?;
                let value = parse_value(store, slot.ty, raw)?;
                positional.push(value.clone());
                if let Some(name) = slot.name {
                    named.push((name, value));
                }
                cursor = end;
            }
        }
    }
    if cursor != input.len() {
        return Err(PatternError::NoMatch);
    }
    Ok(Bindings { positional, named })
}

pub fn destructure(
    store: &Store,
    pattern: PatternId,
    input: &str,
) -> Result<Bindings, PatternError> {
    match_pattern(store, pattern, input)
}

pub fn compose(
    store: &mut Store,
    left: PatternId,
    right: PatternId,
) -> Result<PatternId, PatternError> {
    let mut parts = store
        .patterns
        .get(left.0 as usize)
        .ok_or(PatternError::UnknownPattern(left.0.to_string()))?
        .parts
        .clone();
    let right_parts = store
        .patterns
        .get(right.0 as usize)
        .ok_or(PatternError::UnknownPattern(right.0.to_string()))?
        .parts
        .clone();
    let existing = parts
        .iter()
        .filter_map(|part| match part {
            PatternPart::Slot(slot) => slot.name,
            _ => None,
        })
        .collect::<HashSet<_>>();
    if right_parts.iter().any(|part| matches!(part, PatternPart::Slot(slot) if slot.name.is_some_and(|name| existing.contains(&name)))) { return Err(PatternError::DuplicateBinding("composed slot".to_owned())); }
    let offset = parts
        .iter()
        .filter(|part| matches!(part, PatternPart::Slot(_)))
        .count() as u32;
    parts.extend(right_parts.into_iter().map(|part| match part {
        PatternPart::Slot(mut slot) => {
            slot.position += offset;
            PatternPart::Slot(slot)
        }
        other => other,
    }));
    Ok(store.alloc_pattern(Pattern {
        name: None,
        parts,
        span: store.source.span(0, store.source.text.len()),
    }))
}

pub fn enumerate_slots(store: &Store, pattern: PatternId) -> impl Iterator<Item = &Slot> {
    store.patterns[pattern.0 as usize]
        .parts
        .iter()
        .filter_map(|part| match part {
            PatternPart::Slot(slot) => Some(slot),
            _ => None,
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedPath {
    pub text: String,
    pub leaf: TypeId,
}

pub fn enumerate_paths(store: &Store, root: TypeId) -> Vec<TypedPath> {
    let mut output = Vec::new();
    walk_paths(store, root, String::new(), &mut output, &mut HashSet::new());
    output
}

fn walk_paths(
    store: &Store,
    id: TypeId,
    prefix: String,
    output: &mut Vec<TypedPath>,
    visited: &mut HashSet<(TypeId, usize)>,
) {
    let id = unwrap_alias(store, id);
    if !visited.insert((id, prefix.len())) {
        return;
    }
    match &store.types[id.0 as usize] {
        Type::Record(record) => {
            for field in &record.fields {
                let name = store.symbols.resolve(field.name);
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}.{name}")
                };
                output.push(TypedPath {
                    text: path.clone(),
                    leaf: field.ty,
                });
                walk_paths(store, field.ty, path, output, visited);
            }
        }
        Type::Array(item) => {
            let path = format!("{prefix}[*]");
            output.push(TypedPath {
                text: path.clone(),
                leaf: *item,
            });
            walk_paths(store, *item, path, output, visited);
        }
        Type::Map { value, .. } => {
            let path = format!("{prefix}{{key}}");
            output.push(TypedPath {
                text: path.clone(),
                leaf: *value,
            });
            walk_paths(store, *value, path, output, visited);
        }
        Type::Optional(inner) => walk_paths(store, *inner, prefix, output, visited),
        Type::Union(items) => {
            for item in items {
                walk_paths(store, *item, prefix.clone(), output, visited);
            }
        }
        _ => {}
    }
}

fn unwrap_alias(store: &Store, mut id: TypeId) -> TypeId {
    while let Type::Alias { target, .. } = store.types[id.0 as usize] {
        if target == id {
            break;
        }
        id = target;
    }
    id
}

fn validate(store: &Store, id: TypeId, value: &Value) -> Result<(), PatternError> {
    if type_accepts(store, id, value) {
        Ok(())
    } else {
        Err(PatternError::TypeMismatch(store.type_name(id)))
    }
}
fn type_accepts(store: &Store, id: TypeId, value: &Value) -> bool {
    match &store.types[id.0 as usize] {
        Type::Alias { target, .. } => type_accepts(store, *target, value),
        Type::Primitive(Primitive::String) => matches!(value, Value::String(_)),
        Type::Primitive(Primitive::Int) => matches!(value, Value::Int(_)),
        Type::Primitive(Primitive::Bool) => matches!(value, Value::Bool(_)),
        Type::Literal(expected) => expected == value,
        Type::Union(items) => items.iter().any(|item| type_accepts(store, *item, value)),
        Type::Optional(inner) => type_accepts(store, *inner, value),
        _ => true,
    }
}
fn parse_value(store: &Store, id: TypeId, raw: &str) -> Result<Value, PatternError> {
    match &store.types[id.0 as usize] {
        Type::Alias { target, .. } => parse_value(store, *target, raw),
        Type::Primitive(Primitive::Int) => raw
            .parse()
            .map(Value::Int)
            .map_err(|_| PatternError::TypeMismatch("Int".to_owned())),
        Type::Primitive(Primitive::Bool) => raw
            .parse()
            .map(Value::Bool)
            .map_err(|_| PatternError::TypeMismatch("Bool".to_owned())),
        Type::Union(items) => items
            .iter()
            .find_map(|item| parse_value(store, *item, raw).ok())
            .ok_or_else(|| PatternError::TypeMismatch("union".to_owned())),
        _ => Ok(Value::String(raw.to_owned())),
    }
}
fn value_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Int(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
    }
}

pub fn symbol_name(store: &Store, symbol: Symbol) -> &str {
    store.symbols.resolve(symbol)
}
