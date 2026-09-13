//! Argument slots, classification and assignment. Port of
//! `0_lowerer.pl:1226-1425`, `:1737-1741` and `:1785-1832`.

use super::cx::Cx;
use super::forms;
use super::kernel;
use crate::_6_eval::term::TermId;

#[derive(Clone, Debug)]
pub struct Slot {
    pub index: i64,
    /// `None` is the atom `none` at `:1276`.
    pub label: Option<TermId>,
}

#[derive(Clone, Debug)]
pub enum Classified {
    Named { index: i64, node: TermId },
    Positional(TermId),
}

#[derive(Clone, Debug)]
pub struct Assigned {
    pub index: i64,
    pub node: TermId,
}

/// The callable a name resolved to: `target(Owner)` or `kernel(Name)`.
#[derive(Clone, Debug)]
pub enum Callable {
    Target(TermId),
    Kernel(String),
}

impl Callable {
    pub fn term(&self, cx: &mut Cx) -> TermId {
        match self {
            Callable::Target(owner) => cx.compound("target", vec![*owner]),
            Callable::Kernel(name) => {
                let atom = cx.atom(name);
                cx.compound("kernel", vec![atom])
            }
        }
    }
}

/// `:1265`.
pub fn callable_slots(cx: &mut Cx, callable: &Callable, arity: i64) -> Vec<Slot> {
    (0..arity)
        .map(|index| {
            let label = match callable {
                Callable::Target(owner) => cx.edges.callable_slot(cx.u, *owner, index),
                Callable::Kernel(name) => {
                    kernel::kernel_slot_label(name, index as u32).map(|l| cx.atom(l))
                }
            };
            Slot { index, label }
        })
        .collect()
}

/// `:1737`.
pub fn exclude_slot(slots: &[Slot], index: Option<i64>) -> Vec<Slot> {
    slots
        .iter()
        .filter(|slot| Some(slot.index) != index)
        .cloned()
        .collect()
}

/// `:1314`. The first classification diagnostic aborts the call.
pub fn classify(cx: &mut Cx, nodes: &[TermId], slots: &[Slot]) -> Result<Vec<Classified>, TermId> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut first: Option<TermId> = None;
    for node in nodes {
        match classify_one(cx, *node, slots) {
            Ok(classified) => out.push(classified),
            Err(reason) => {
                if first.is_none() {
                    first = Some(reason);
                }
            }
        }
    }
    match first {
        Some(reason) => Err(reason),
        None => Ok(out),
    }
}

/// `:1321`. The error is the bare reason term; the caller wraps it.
fn classify_one(cx: &mut Cx, node: TermId, slots: &[Slot]) -> Result<Classified, TermId> {
    if let Some(parsed) = forms::node(cx.u, node) {
        if let Some(items) = forms::form(cx.u, parsed.payload) {
            let colon = forms::form_head_atom(cx.u, &items)
                .is_some_and(|a| cx.u.functor_or_atom(a).is_some_and(|(n, _)| n == ":"));
            if colon && items.len() >= 2 {
                let label_node = forms::node(cx.u, items[1]);
                let label = label_node
                    .as_ref()
                    .and_then(|l| forms::atom_name(cx.u, l.payload));
                if let Some(label) = label {
                    if items.len() == 3 {
                        return match slots.iter().find(|s| s.label == Some(label)) {
                            Some(slot) => Ok(Classified::Named {
                                index: slot.index,
                                node: items[2],
                            }),
                            None => Err(cx.compound("unknown_argument_label", vec![label])),
                        };
                    }
                    let count = cx.int(items.len() as i64 - 2);
                    return Err(
                        cx.compound("named_argument_requires_one_value", vec![label, count])
                    );
                }
            }
        }
        // :1330. A variable whose name is a slot label is a pun for that slot.
        if let Some((_, name)) = forms::variable(cx.u, parsed.payload) {
            let underscore = cx.atom("_");
            if name != underscore {
                if let Some(slot) = slots.iter().find(|s| s.label == Some(name)) {
                    return Ok(Classified::Named {
                        index: slot.index,
                        node,
                    });
                }
            }
        }
    }
    Ok(Classified::Positional(node))
}

/// `:1342`.
pub fn assign(
    cx: &mut Cx,
    classified: &[Classified],
    slots: &[Slot],
    reserved: &[i64],
) -> Result<Vec<Assigned>, TermId> {
    let mut all: Vec<i64> = reserved.to_vec();
    all.extend(classified.iter().filter_map(|c| match c {
        Classified::Named { index, .. } => Some(*index),
        _ => None,
    }));
    let mut sorted = all.clone();
    sorted.sort_unstable();
    if let Some(duplicate) = sorted.windows(2).find(|w| w[0] == w[1]).map(|w| w[0]) {
        let index = cx.int(duplicate);
        return Err(cx.compound("duplicate_argument_slot", vec![index]));
    }
    let mut available: Vec<i64> = slots
        .iter()
        .map(|s| s.index)
        .filter(|i| !all.contains(i))
        .collect();
    available.reverse();
    let mut out = Vec::with_capacity(classified.len());
    for item in classified {
        match item {
            Classified::Named { index, node } => out.push(Assigned {
                index: *index,
                node: *node,
            }),
            Classified::Positional(node) => match available.pop() {
                Some(index) => out.push(Assigned { index, node: *node }),
                None => return Err(cx.atom("too_many_arguments")),
            },
        }
    }
    Ok(out)
}

/// `:1418`. A slot with no bound value becomes a deterministic fresh variable.
pub fn fill(cx: &mut Cx, slots: &[Slot], bound: &[(i64, TermId)], node: TermId) -> Vec<TermId> {
    slots
        .iter()
        .map(|slot| match bound.iter().find(|(i, _)| *i == slot.index) {
            Some((_, value)) => *value,
            None => {
                let index = cx.int(slot.index);
                let omitted = cx.compound("omitted", vec![node, index]);
                cx.compound("var", vec![omitted])
            }
        })
        .collect()
}

/// `:1802`.
pub fn missing(slots: &[Slot], bound: &[(i64, TermId)]) -> Vec<Slot> {
    slots
        .iter()
        .filter(|slot| !bound.iter().any(|(i, _)| *i == slot.index))
        .cloned()
        .collect()
}

/// `:1810`. Every slot must be bound; the caller has checked `missing`.
pub fn ordered(slots: &[Slot], bound: &[(i64, TermId)]) -> Vec<TermId> {
    slots
        .iter()
        .filter_map(|slot| {
            bound
                .iter()
                .find(|(i, _)| *i == slot.index)
                .map(|(_, v)| *v)
        })
        .collect()
}

/// `:903`.
pub fn positions_except(arity: i64, except: Option<i64>) -> Vec<i64> {
    (0..arity).filter(|i| Some(*i) != except).collect()
}

/// `:1831`.
pub fn positions_subset(key_set: &[i64], supplied: &[i64]) -> bool {
    key_set.iter().all(|p| supplied.contains(p))
}
