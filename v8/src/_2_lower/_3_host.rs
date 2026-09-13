//! `Host` bind targets and the rules their metadata reservation emits.
//! Port of `0_lowerer.pl:783-866` and `:625-650`.

use super::declare::{
    diagnostic, finish_constructor_target, lower_bind_list, reason, Fallible, Target,
};
use super::forms;
use crate::_6_eval::term::{TermId, Universe};

/// `:783`. `items` is the whole `(Host Impl Input Output)` form.
pub fn lower_host_target(
    u: &mut Universe,
    items: &[TermId],
    host_node_id: TermId,
    owner: TermId,
    module_identity: TermId,
) -> Fallible<Target> {
    let implementation = host_implementation(u, items[1], owner)?;
    let input_bindings = host_product_bindings(u, items[2])?;
    let output_bindings = host_product_bindings(u, items[3])?;
    let mut ports = host_port_specs(u, &input_bindings, "Input")?;
    ports.extend(host_port_specs(u, &output_bindings, "Output")?);
    let mut bindings = input_bindings;
    bindings.extend(output_bindings);
    let declared = lower_bind_list(u, &bindings, owner, module_identity)?;
    let mut target = finish_constructor_target(u, declared, host_node_id, owner, "product");
    // :857. The host reservation rides in front of the product's own.
    let marker = u.compound("host_marker", vec![host_node_id]);
    let port_list = u.list(&ports);
    let metadata = u.compound(
        "host_metadata",
        vec![implementation, port_list, host_node_id],
    );
    let kind = u.atom("host");
    let reservation = u.compound("reservation", vec![owner, marker, metadata, kind]);
    target.declared.reservations.insert(0, reservation);
    Ok(target)
}

/// `:799`.
fn host_implementation(u: &mut Universe, node: TermId, owner: TermId) -> Fallible<TermId> {
    let parsed = forms::node(u, node);
    if let Some(parsed) = parsed {
        if let Some(name) = forms::atom_name(u, parsed.payload) {
            return Ok(u.compound("name", vec![owner, name]));
        }
        let r = reason(u, "host_implementation_must_be_name");
        return Err(diagnostic(u, parsed.id, r));
    }
    let r = reason(u, "host_implementation_must_be_name");
    Err(diagnostic(u, node, r))
}

/// `:801`.
fn host_product_bindings(u: &mut Universe, node: TermId) -> Fallible<Vec<TermId>> {
    if let Some(parsed) = forms::node(u, node) {
        if let Some(items) = forms::form(u, parsed.payload) {
            if let Some(head) = forms::form_head_atom(u, &items) {
                if u.functor_or_atom(head).is_some_and(|(n, _)| n == "*") {
                    return Ok(items[1..].to_vec());
                }
            }
        }
        let r = reason(u, "host_ports_must_be_product");
        return Err(diagnostic(u, parsed.id, r));
    }
    let r = reason(u, "host_ports_must_be_product");
    Err(diagnostic(u, node, r))
}

/// `:827`. `host_port(Label, Direction, LabelNodeId)`.
fn host_port_specs(
    u: &mut Universe,
    bindings: &[TermId],
    direction: &str,
) -> Fallible<Vec<TermId>> {
    let mut ports = Vec::with_capacity(bindings.len());
    for bind in bindings {
        let label_node = forms::edge_bind_form(u, *bind).map(|b| b.label);
        let parsed = label_node.and_then(|l| forms::node(u, l));
        let label = parsed.as_ref().and_then(|p| forms::atom_name(u, p.payload));
        match (parsed, label) {
            (Some(parsed), Some(label)) => {
                let direction_term = u.atom(direction);
                ports.push(u.compound("host_port", vec![label, direction_term, parsed.id]));
            }
            _ => {
                let node = forms::node_id(u, *bind).unwrap_or(*bind);
                let r = reason(u, "host_port_label_must_be_atom");
                return Err(diagnostic(u, node, r));
            }
        }
    }
    Ok(ports)
}

/// `:625`. One `Hosted` row plus one `HostPort` row per port.
pub fn host_metadata_rules(
    u: &mut Universe,
    owner: TermId,
    metadata: TermId,
    rule_index: i64,
) -> Option<(Vec<TermId>, Vec<TermId>)> {
    let (name, args) = u.functor(metadata)?;
    if name != "host_metadata" || args.len() != 3 {
        return None;
    }
    let (implementation, ports, host_node_id) = (args[0], args[1], args[2]);
    let ports = u.as_list(ports)?;
    let hosted_name = u.atom("Hosted");
    let relation = u.compound("name", vec![owner, hosted_name]);
    let owner_ref = u.compound("ref", vec![owner]);
    let arguments = u.list(&[owner_ref, implementation]);
    let call = u.compound("call", vec![relation, arguments]);
    let empty = u.empty_list();
    let mut rules = vec![u.compound("rule", vec![call, empty])];
    let mut origins = vec![rule_origin(u, rule_index, host_node_id)];
    for (offset, port) in ports.iter().enumerate() {
        let (_, port_args) = u.functor(*port)?;
        let (label, direction, node_id) = (port_args[0], port_args[1], port_args[2]);
        let port_name = u.atom("HostPort");
        let relation = u.compound("name", vec![owner, port_name]);
        let owner_ref = u.compound("ref", vec![owner]);
        let label_const = u.compound("const", vec![label]);
        let direction_name = u.compound("name", vec![owner, direction]);
        let arguments = u.list(&[owner_ref, label_const, direction_name]);
        let call = u.compound("call", vec![relation, arguments]);
        let empty = u.empty_list();
        rules.push(u.compound("rule", vec![call, empty]));
        origins.push(rule_origin(u, rule_index + 1 + offset as i64, node_id));
    }
    Some((rules, origins))
}

pub fn rule_origin(u: &mut Universe, rule_index: i64, node: TermId) -> TermId {
    let index = u.int(rule_index);
    let rule = u.compound("rule", vec![index]);
    u.compound("origin", vec![rule, node])
}

/// `:652`.
pub fn indexed_goal_origins(u: &mut Universe, nodes: &[TermId], rule_index: i64) -> Vec<TermId> {
    nodes
        .iter()
        .enumerate()
        .map(|(goal_index, node)| {
            let rule = u.int(rule_index);
            let goal = u.int(goal_index as i64);
            let key = u.compound("goal", vec![rule, goal]);
            u.compound("origin", vec![key, *node])
        })
        .collect()
}

/// `:536`. One origin per rule in a block that shares a source node.
pub fn indexed_empty_rule_origins(
    u: &mut Universe,
    count: usize,
    rule_index: i64,
    node: TermId,
) -> Vec<TermId> {
    (0..count)
        .map(|offset| rule_origin(u, rule_index + offset as i64, node))
        .collect()
}
