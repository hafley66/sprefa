//! Passes 1 and 2: mint every nested owner, reserve every bind in that owner.
//! Port of `0_lowerer.pl:672-982`.

use super::forms;
use crate::_6_eval::term::{TermId, Universe};

#[derive(Default, Debug)]
pub struct Declared {
    pub nodes: Vec<TermId>,
    pub edges: Vec<TermId>,
    pub origins: Vec<TermId>,
    pub reservations: Vec<TermId>,
}

impl Declared {
    pub fn extend(&mut self, other: Declared) {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
        self.origins.extend(other.origins);
        self.reservations.extend(other.reservations);
    }
}

/// The seven-tuple `lower_target/4` returns (`:748`).
pub struct Target {
    pub term: TermId,
    pub kind: &'static str,
    pub declared: Declared,
}

pub type Fallible<T> = Result<T, TermId>;

pub fn diagnostic(u: &mut Universe, node: TermId, reason: TermId) -> TermId {
    let lower = u.atom("lower");
    u.compound("diagnostic", vec![lower, node, reason])
}

pub fn reason(u: &mut Universe, name: &str) -> TermId {
    u.atom(name)
}

/// `:673`. Only bind forms advance the ordinal.
pub fn lower_declarations(
    u: &mut Universe,
    items: &[TermId],
    owner: TermId,
    module_identity: TermId,
) -> Fallible<Declared> {
    let mut out = Declared::default();
    let mut index: i64 = 0;
    for item in items {
        if forms::bind_form(u, *item).is_some() {
            let call = Call::Bind {
                bind_node: *item,
                owner,
                index,
            };
            let Ret::Declared(declared) = run(u, module_identity, call)? else {
                unreachable!("a bind returns its declarations")
            };
            out.extend(declared);
            index += 1;
        }
    }
    Ok(out)
}

/// One function of the declaration passes, started with its arguments.
/// `module_identity` is the same for every call of one run.
enum Call {
    /// `:701`.
    Bind { bind_node: TermId, owner: TermId, index: i64 },
    /// `:938`.
    EdgeBind { bind_node: TermId, owner: TermId, index: i64 },
    /// `:748`.
    Target { target_node: TermId, owner: TermId },
    /// `:917`, a bind list.
    BindList { bindings: Vec<TermId>, owner: TermId },
    /// A generic application as a body item, from bind index `index`.
    ApplicationItem { item: TermId, owner: TermId, index: i64 },
}

/// What a finished call hands to the frame below it.
enum Ret {
    Declared(Declared),
    Target(Target),
    /// An application item's declarations and the next bind index.
    Item(Declared, i64),
}

/// The rest of a call that waits on a nested one.
enum Frame {
    Call(Call),
    /// `:728`, on the target's return.
    FinishBind { bind_node: TermId, owner: TermId, name: TermId, index: i64 },
    /// `:965`, on the target's return.
    FinishCompound { bind_node: TermId, owner: TermId, label: TermId, index: i64 },
    /// `:868`, on the bind list's return.
    FinishConstructor { node_id: TermId, owner: TermId, kind: &'static str },
    BindList(BindList),
    ApplicationItem(ApplicationItem),
}

enum Outcome {
    Done(Ret),
    /// Run `call`, then resume `then` with its return.
    Await(Frame, Call),
    /// Resume the frame with nothing returned.
    Again(Frame),
}

/// Runs one call and every call it nests on an explicit stack; the first
/// diagnostic ends the run.
fn run(u: &mut Universe, module_identity: TermId, call: Call) -> Fallible<Ret> {
    let mut stack = vec![Frame::Call(call)];
    let mut returned: Option<Ret> = None;
    while let Some(frame) = stack.pop() {
        let outcome = match frame {
            Frame::Call(call) => start(u, module_identity, call)?,
            Frame::FinishBind {
                bind_node,
                owner,
                name,
                index,
            } => {
                let target = returned_target(returned.take());
                Outcome::Done(Ret::Declared(finish_bind(
                    u, target, bind_node, owner, name, index,
                )))
            }
            Frame::FinishCompound {
                bind_node,
                owner,
                label,
                index,
            } => {
                let target = returned_target(returned.take());
                Outcome::Done(Ret::Declared(finish_compound_edge_bind(
                    u, target, bind_node, owner, label, index,
                )))
            }
            Frame::FinishConstructor {
                node_id,
                owner,
                kind,
            } => {
                let Some(Ret::Declared(declared)) = returned.take() else {
                    unreachable!("a bind list returns its declarations")
                };
                Outcome::Done(Ret::Target(finish_constructor_target(
                    u, declared, node_id, owner, kind,
                )))
            }
            Frame::BindList(list) => list.resume(u, returned.take())?,
            Frame::ApplicationItem(item) => item.resume(u, returned.take())?,
        };
        match outcome {
            Outcome::Done(ret) => returned = Some(ret),
            Outcome::Await(then, call) => {
                stack.push(then);
                stack.push(Frame::Call(call));
            }
            Outcome::Again(frame) => stack.push(frame),
        }
    }
    Ok(returned.expect("the outermost call returns"))
}

fn returned_target(returned: Option<Ret>) -> Target {
    let Some(Ret::Target(target)) = returned else {
        unreachable!("a target call returns its target")
    };
    target
}

fn start(u: &mut Universe, module_identity: TermId, call: Call) -> Fallible<Outcome> {
    match call {
        Call::Bind {
            bind_node,
            owner,
            index,
        } => lower_bind(u, bind_node, owner, index),
        Call::EdgeBind {
            bind_node,
            owner,
            index,
        } => lower_edge_bind(u, bind_node, owner, index),
        Call::Target { target_node, owner } => lower_target(u, target_node, owner, module_identity),
        Call::BindList { bindings, owner } => BindList {
            bindings,
            owner,
            next: 0,
            index: 0,
            out: Declared::default(),
            pending: Pending::Nothing,
        }
        .resume(u, None),
        Call::ApplicationItem { item, owner, index } => {
            let Some(node) = forms::node(u, item) else {
                return Ok(Outcome::Done(Ret::Item(Declared::default(), index)));
            };
            let items = forms::form(u, node.payload).unwrap_or_default();
            ApplicationItem {
                node_id: node.id,
                arguments: Vec::with_capacity(items.len().saturating_sub(1)),
                items,
                owner,
                index,
                next: 1,
                out: Declared::default(),
                group: None,
                pending: None,
            }
            .resume(u, None)
        }
    }
}

/// `:701`.
fn lower_bind(u: &mut Universe, bind_node: TermId, owner: TermId, index: i64) -> Fallible<Outcome> {
    let Some(bind) = forms::bind_form(u, bind_node) else {
        let node = forms::node_id(u, bind_node).unwrap_or(bind_node);
        let r = reason(u, "expected_bind");
        return Err(diagnostic(u, node, r));
    };
    if forms::expression_bind_target(u, bind.target) || forms::literal_bind_target(u, bind.target) {
        return Ok(Outcome::Done(Ret::Declared(finish_derived_bind(
            u,
            bind.bind_node,
            owner,
            bind.name,
            bind.target,
            index,
        ))));
    }
    Ok(Outcome::Await(
        Frame::FinishBind {
            bind_node: bind.bind_node,
            owner,
            name: bind.name,
            index,
        },
        Call::Target {
            target_node: bind.target,
            owner,
        },
    ))
}

/// `:719`.
fn finish_derived_bind(
    u: &mut Universe,
    bind_node_id: TermId,
    owner: TermId,
    name: TermId,
    target_node: TermId,
    index: i64,
) -> Declared {
    let index_term = u.int(index);
    let edge_target = u.compound("deferred_expression", vec![target_node]);
    let edge = u.compound("pending_edge", vec![owner, name, edge_target, index_term]);
    let reservation_target = u.compound(
        "deferred_expression",
        vec![target_node, bind_node_id, index_term],
    );
    let kind = u.atom("expression");
    let reservation = u.compound("reservation", vec![owner, name, reservation_target, kind]);
    let origin = edge_origin(u, owner, name, index, bind_node_id);
    Declared {
        nodes: vec![],
        edges: vec![edge],
        origins: vec![origin],
        reservations: vec![reservation],
    }
}

pub fn edge_origin(
    u: &mut Universe,
    owner: TermId,
    name: TermId,
    index: i64,
    node: TermId,
) -> TermId {
    let index_term = u.int(index);
    let edge = u.compound("edge", vec![owner, name, index_term]);
    u.compound("origin", vec![edge, node])
}

/// `:728`.
fn finish_bind(
    u: &mut Universe,
    target: Target,
    bind_node_id: TermId,
    owner: TermId,
    name: TermId,
    index: i64,
) -> Declared {
    let index_term = u.int(index);
    let edge = u.compound("pending_edge", vec![owner, name, target.term, index_term]);
    let kind = u.atom(target.kind);
    let reservation = u.compound("reservation", vec![owner, name, target.term, kind]);
    let mut origins = vec![edge_origin(u, owner, name, index, bind_node_id)];
    // :740. A product bind also records the relation it declares.
    if target.kind == "product" {
        if let Some(inner) = u.unary(target.term, "target") {
            let relation = u.compound("relation", vec![inner]);
            origins.push(u.compound("origin", vec![relation, bind_node_id]));
        }
    }
    let mut declared = target.declared;
    let mut edges = vec![edge];
    edges.append(&mut declared.edges);
    origins.extend(declared.origins);
    let mut reservations = vec![reservation];
    reservations.append(&mut declared.reservations);
    Declared {
        nodes: declared.nodes,
        edges,
        origins,
        reservations,
    }
}

/// `:748`. Eight clauses in source order.
fn lower_target(
    u: &mut Universe,
    target_node: TermId,
    parent_owner: TermId,
    module_identity: TermId,
) -> Fallible<Outcome> {
    let done = |target: Target| Ok(Outcome::Done(Ret::Target(target)));
    let Some(node) = forms::node(u, target_node) else {
        let r = reason(u, "unsupported_bind_target");
        return Err(diagnostic(u, target_node, r));
    };
    if let Some(items) = forms::form(u, node.payload) {
        if let Some(head) = forms::form_head_atom(u, &items) {
            let head_name = u
                .functor_or_atom(head)
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
            match head_name.as_str() {
                "*" | "+" => {
                    let kind = if head_name == "*" { "product" } else { "sum" };
                    let owner = u.compound("owner", vec![module_identity, node.id]);
                    return Ok(Outcome::Await(
                        Frame::FinishConstructor {
                            node_id: node.id,
                            owner,
                            kind,
                        },
                        Call::BindList {
                            bindings: items[1..].to_vec(),
                            owner,
                        },
                    ));
                }
                _ => {}
            }
        }
        if items.is_empty() {
            // :774. The empty form is the reference `'()'`.
            let empty = u.atom("()");
            let term = forms::written_name(u, parent_owner, empty);
            return done(Target {
                term,
                kind: "reference",
                declared: Declared::default(),
            });
        }
        let r = reason(u, "unsupported_bind_target");
        return Err(diagnostic(u, node.id, r));
    }
    if let Some(name) = forms::atom_name(u, node.payload) {
        let term = forms::written_name(u, parent_owner, name);
        return done(Target {
            term,
            kind: "reference",
            declared: Declared::default(),
        });
    }
    if let Some(value) = forms::literal_value(u, node.payload) {
        let term = u.compound("const", vec![value]);
        return done(Target {
            term,
            kind: "literal",
            declared: Declared::default(),
        });
    }
    if forms::variable(u, node.payload).is_some() {
        let r = reason(u, "variable_bind_target");
        return Err(diagnostic(u, node.id, r));
    }
    let r = reason(u, "unsupported_bind_target");
    Err(diagnostic(u, node.id, r))
}

/// `:868`.
pub fn finish_constructor_target(
    u: &mut Universe,
    mut declared: Declared,
    node_id: TermId,
    owner: TermId,
    kind: &'static str,
) -> Target {
    let classifier = u.compound(kind, vec![owner]);
    let identity = u.compound("node", vec![owner]);
    let mut nodes = vec![identity, classifier];
    nodes.append(&mut declared.nodes);
    let node_origin = u.compound("node", vec![owner]);
    let origin = u.compound("origin", vec![node_origin, node_id]);
    let mut origins = vec![origin];
    origins.append(&mut declared.origins);
    let term = u.compound("target", vec![owner]);
    Target {
        term,
        kind,
        declared: Declared {
            nodes,
            edges: declared.edges,
            origins,
            reservations: declared.reservations,
        },
    }
}


/// The head node of a body item `(Head Argument..)` whose head is an atom other
/// than `:`, `*` and `+`, or a dotted path.
fn application_head(u: &Universe, item: TermId) -> Option<TermId> {
    let node = forms::node(u, item)?;
    let items = forms::form(u, node.payload)?;
    let head = *items.first()?;
    if super::express::path_segments(u, head).is_some() {
        return Some(head);
    }
    let atom = forms::form_head_atom(u, &items)?;
    let text = u.functor_or_atom(atom).map(|(n, _)| n)?;
    (!matches!(text, ":" | "*" | "+")).then_some(head)
}

/// What a bind list waits on.
#[derive(Clone, Copy)]
enum Pending {
    Nothing,
    EdgeBind,
    Item,
}

/// `:917`. A bind list part way through `bindings`.
struct BindList {
    bindings: Vec<TermId>,
    owner: TermId,
    next: usize,
    index: i64,
    out: Declared,
    pending: Pending,
}

impl BindList {
    fn resume(mut self, u: &mut Universe, returned: Option<Ret>) -> Fallible<Outcome> {
        match (self.pending, returned) {
            (Pending::EdgeBind, Some(Ret::Declared(declared))) => {
                self.out.extend(declared);
                self.index += 1;
            }
            (Pending::Item, Some(Ret::Item(declared, index))) => {
                self.out.extend(declared);
                self.index = index;
            }
            (Pending::Nothing, None) => {}
            _ => unreachable!("a bind list resumes with the return it waits on"),
        }
        let Some(&bind) = self.bindings.get(self.next) else {
            return Ok(Outcome::Done(Ret::Declared(self.out)));
        };
        self.next += 1;
        let call = if application_head(u, bind).is_some() {
            self.pending = Pending::Item;
            Call::ApplicationItem {
                item: bind,
                owner: self.owner,
                index: self.index,
            }
        } else {
            self.pending = Pending::EdgeBind;
            Call::EdgeBind {
                bind_node: bind,
                owner: self.owner,
                index: self.index,
            }
        };
        Ok(Outcome::Await(Frame::BindList(self), call))
    }
}

/// The binds of one `(* ..)` argument, with the refs of those already lowered.
struct Group {
    members: Vec<TermId>,
    next: usize,
    edges: Vec<TermId>,
}

/// A generic application as a body item. A bind argument lands on `owner` at
/// the next index and passes `ref(edge(Owner, Label))`; a `(* ..)` argument
/// lands each bind in order and passes the tuple of their refs. Derived rules
/// build the instance from the `application` reservation.
struct ApplicationItem {
    node_id: TermId,
    items: Vec<TermId>,
    owner: TermId,
    index: i64,
    next: usize,
    arguments: Vec<TermId>,
    out: Declared,
    group: Option<Group>,
    /// The name of the bind argument whose edge bind is running.
    pending: Option<TermId>,
}

impl ApplicationItem {
    /// One step: an argument, a group member, a closed group, or the reservation.
    fn resume(mut self, u: &mut Universe, returned: Option<Ret>) -> Fallible<Outcome> {
        if let Some(name) = self.pending.take() {
            let Some(Ret::Declared(declared)) = returned else {
                unreachable!("an edge bind returns its declarations")
            };
            self.out.extend(declared);
            self.index += 1;
            let edge = u.compound("edge", vec![self.owner, name]);
            let edge = u.compound("ref", vec![edge]);
            match &mut self.group {
                Some(group) => group.edges.push(edge),
                None => self.arguments.push(edge),
            }
            return Ok(Outcome::Again(Frame::ApplicationItem(self)));
        }
        if let Some(group) = &mut self.group {
            let Some(&member) = group.members.get(group.next) else {
                let tuple = u.atom("tuple");
                let list = u.list(&group.edges);
                self.arguments.push(u.compound("application", vec![tuple, list]));
                self.group = None;
                return Ok(Outcome::Again(Frame::ApplicationItem(self)));
            };
            group.next += 1;
            let Some(bind) = forms::bind_form(u, member) else {
                let r = reason(u, "expected_bind");
                let at = forms::node_id(u, member).unwrap_or(member);
                return Err(diagnostic(u, at, r));
            };
            return Ok(self.edge_bind(member, bind.name));
        }
        let Some(&argument) = self.items.get(self.next) else {
            let list = u.list(&self.arguments);
            let target = u.compound("application", vec![self.items[0], list]);
            let kind = u.atom("application");
            self.out
                .reservations
                .push(u.compound("reservation", vec![self.owner, self.node_id, target, kind]));
            return Ok(Outcome::Done(Ret::Item(self.out, self.index)));
        };
        self.next += 1;
        if let Some(bind) = forms::bind_form(u, argument) {
            return Ok(self.edge_bind(argument, bind.name));
        }
        let group = forms::node(u, argument)
            .and_then(|inner| forms::form(u, inner.payload))
            .filter(|inner| {
                forms::form_head_atom(u, inner)
                    .is_some_and(|head| u.functor_or_atom(head).is_some_and(|(n, _)| n == "*"))
            });
        let Some(group) = group else {
            let shape = u.atom("generic_application");
            let reason = u.compound("not_built_yet", vec![shape]);
            let at = forms::node_id(u, argument).unwrap_or(argument);
            return Err(diagnostic(u, at, reason));
        };
        self.group = Some(Group {
            edges: Vec::with_capacity(group.len().saturating_sub(1)),
            members: group,
            next: 1,
        });
        Ok(Outcome::Again(Frame::ApplicationItem(self)))
    }

    /// A bind argument lowered onto `owner` at the next index.
    fn edge_bind(mut self, bind_node: TermId, name: TermId) -> Outcome {
        self.pending = Some(name);
        let call = Call::EdgeBind {
            bind_node,
            owner: self.owner,
            index: self.index,
        };
        Outcome::Await(Frame::ApplicationItem(self), call)
    }
}

/// `:938`.
fn lower_edge_bind(u: &mut Universe, bind_node: TermId, owner: TermId, index: i64) -> Fallible<Outcome> {
    let Some(bind) = forms::edge_bind_form(u, bind_node) else {
        let node = forms::node_id(u, bind_node).unwrap_or(bind_node);
        let r = reason(u, "expected_bind");
        return Err(diagnostic(u, node, r));
    };
    let Some(label) = forms::node(u, bind.label) else {
        let r = reason(u, "edge_label_must_be_atom_or_expression");
        return Err(diagnostic(u, bind.label, r));
    };
    if forms::atom_name(u, label.payload).is_some() {
        return lower_bind(u, bind_node, owner, index);
    }
    if forms::form(u, label.payload).is_some() {
        // :958. The compound-label path accepts the same target forms.
        if forms::expression_bind_target(u, bind.target) {
            let target = Target {
                term: u.compound("deferred_expression", vec![bind.target]),
                kind: "reference",
                declared: Declared::default(),
            };
            return Ok(Outcome::Done(Ret::Declared(finish_compound_edge_bind(
                u,
                target,
                bind.bind_node,
                owner,
                bind.label,
                index,
            ))));
        }
        return Ok(Outcome::Await(
            Frame::FinishCompound {
                bind_node: bind.bind_node,
                owner,
                label: bind.label,
                index,
            },
            Call::Target {
                target_node: bind.target,
                owner,
            },
        ));
    }
    let r = reason(u, "edge_label_must_be_atom_or_expression");
    Err(diagnostic(u, label.id, r))
}

/// `:965`.
fn finish_compound_edge_bind(
    u: &mut Universe,
    target: Target,
    bind_node_id: TermId,
    owner: TermId,
    label_node: TermId,
    index: i64,
) -> Declared {
    let marker = u.compound("compound_label", vec![bind_node_id]);
    let index_term = u.int(index);
    let edge_target = u.compound("deferred_compound_edge", vec![label_node, target.term]);
    let edge = u.compound("pending_edge", vec![owner, marker, edge_target, index_term]);
    let reservation_target = u.compound(
        "derived_compound_edge",
        vec![label_node, target.term, bind_node_id, index_term],
    );
    let kind = u.atom("compound_edge");
    let reservation = u.compound("reservation", vec![owner, marker, reservation_target, kind]);
    let origin = edge_origin(u, owner, marker, index, bind_node_id);
    let mut declared = target.declared;
    let mut edges = vec![edge];
    edges.append(&mut declared.edges);
    let mut origins = vec![origin];
    origins.extend(declared.origins);
    let mut reservations = vec![reservation];
    reservations.append(&mut declared.reservations);
    Declared {
        nodes: declared.nodes,
        edges,
        origins,
        reservations,
    }
}
