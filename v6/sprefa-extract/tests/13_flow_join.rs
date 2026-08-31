//! The FlowF pure join: `DfArg` x resolved call edge x `DfParam` folds to
//! `ArgToParam` `FlowEdge`s, and the callee's `Ret` nodes to `RetToCallRes`.
//! No CLI wiring here; dispatching this join inside `resolve_project` is the
//! named follow-up, so the test drives `flow_edges` directly with hand-built
//! inputs.

use sprefa_extract::{
    flow_edges, CallEdgeKind, CallF, ContentId, DfArg, DfF, DfNodeKind, DfParam, ExtractOutput,
    FamilyBundle, FlowEdgeKind, Node, NodeRef, ProjectEdge, ResolutionOrigin, Span,
};

fn span(start: u32, len: u32) -> Span {
    Span { start, len }
}

fn node(span: Span, kind: DfNodeKind) -> Node<DfF> {
    Node::new(span, kind)
}

#[test]
fn join_emits_arg_to_param_and_ret_to_call_res() {
    let caller_blob = ContentId::Blake3([1u8; 32]);
    let callee_blob = ContentId::Blake3([2u8; 32]);

    // Caller: `helper(42)`; the argument value node (span 20..22) feeds the
    // call-res node (span 18..25); the callee identifier sits at 18..24.
    let mut caller_df = FamilyBundle::<DfF>::default();
    let arg_ref = NodeRef(0);
    let call_ref = NodeRef(1);
    caller_df.nodes.push(node(span(20, 2), DfNodeKind::Lit));
    caller_df.nodes.push(node(span(18, 7), DfNodeKind::CallRes));
    caller_df.aux.args.push(DfArg {
        call: call_ref,
        pos: 0,
        arg: arg_ref,
    });
    let mut caller = ExtractOutput::default();
    caller.df = Some(caller_df);

    // Callee: `function helper(x) { return x; }`; parameter at 40..41 and the
    // return node at 45..48, both inside the def span 35..50.
    let mut callee_df = FamilyBundle::<DfF>::default();
    callee_df.nodes.push(node(span(40, 1), DfNodeKind::Param));
    callee_df.nodes.push(node(span(45, 3), DfNodeKind::Ret));
    callee_df.aux.params.push(DfParam {
        node: NodeRef(0),
        pos: 0,
    });
    let mut callee = ExtractOutput::default();
    callee.df = Some(callee_df);

    let edge = ProjectEdge::<CallF>::new(
        NodeRef(0),
        callee_blob.clone(),
        span(35, 15),
        CallEdgeKind::NameResolve,
        ResolutionOrigin::CorpusUnique,
    )
    .with_call_site(span(18, 6));

    let inputs = vec![
        (caller_blob.clone(), &caller),
        (callee_blob.clone(), &callee),
    ];
    let resolved = vec![(caller_blob.clone(), vec![edge])];
    let flows = flow_edges(&inputs, &resolved);

    assert_eq!(flows.len(), 2);

    let arg = &flows[0];
    assert_eq!(arg.kind, FlowEdgeKind::ArgToParam);
    assert_eq!(arg.src_blob, caller_blob);
    assert_eq!(arg.src_span, span(20, 2));
    assert_eq!(arg.dst_blob, callee_blob);
    assert_eq!(arg.dst_span, span(40, 1));

    let ret = &flows[1];
    assert_eq!(ret.kind, FlowEdgeKind::RetToCallRes);
    assert_eq!(ret.src_blob, caller_blob);
    assert_eq!(ret.src_span, span(18, 7));
    assert_eq!(ret.dst_blob, callee_blob);
    assert_eq!(ret.dst_span, span(45, 3));
}

/// A method receiver occupies slot `-1` and has no callee parameter slot, so
/// the join must skip it rather than emit a bogus flow edge.
#[test]
fn receiver_slot_is_skipped() {
    let caller_blob = ContentId::Blake3([3u8; 32]);
    let callee_blob = ContentId::Blake3([4u8; 32]);

    let mut caller_df = FamilyBundle::<DfF>::default();
    let recv_ref = NodeRef(0);
    let call_ref = NodeRef(1);
    caller_df.nodes.push(node(span(10, 2), DfNodeKind::VarRead));
    caller_df.nodes.push(node(span(8, 10), DfNodeKind::CallRes));
    caller_df.aux.args.push(DfArg {
        call: call_ref,
        pos: -1,
        arg: recv_ref,
    });
    let mut caller = ExtractOutput::default();
    caller.df = Some(caller_df);

    let mut callee_df = FamilyBundle::<DfF>::default();
    callee_df.nodes.push(node(span(30, 1), DfNodeKind::Param));
    callee_df.aux.params.push(DfParam {
        node: NodeRef(0),
        pos: 0,
    });
    let mut callee = ExtractOutput::default();
    callee.df = Some(callee_df);

    let edge = ProjectEdge::<CallF>::new(
        NodeRef(0),
        callee_blob.clone(),
        span(25, 10),
        CallEdgeKind::NameResolve,
        ResolutionOrigin::CorpusUnique,
    )
    .with_call_site(span(8, 6));

    let inputs = vec![(caller_blob.clone(), &caller), (callee_blob, &callee)];
    let resolved = vec![(caller_blob, vec![edge])];
    assert!(flow_edges(&inputs, &resolved).is_empty());
}
