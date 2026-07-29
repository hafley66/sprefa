% fixtures/5_flow_value_plane.pl: canned arrivals for the two joins added to
% the flagship flow rail. Hosts are world effects, so the oracle receives the
% same projected rows after the host boundary.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% The resolved edge carries its own caller-site span. Two calls in one caller
% target different callees; matching only caller identity would cross the two
% argument slots. The receiver decoy uses -1 and cannot join a parameter slot.
fixture(flow_arg_param_hop_is_positional_and_site_pinned,
  prog([ col_type(df_arg/6, caller_path, text),
         col_type(df_arg/6, call_start, int),
         col_type(df_arg/6, call_end, int),
         col_type(df_arg/6, pos, int),
         col_type(df_arg/6, arg, text),
         col_type(df_arg/6, arg_end, int),
         col_type(resolved_call_edge/5, caller_path, text),
         col_type(resolved_call_edge/5, call_start, int),
         col_type(resolved_call_edge/5, call_end, int),
         col_type(resolved_call_edge/5, callee_path, text),
         col_type(resolved_call_edge/5, callee_name, text),
         col_type(df_param/4, callee_path, text),
         col_type(df_param/4, param, text),
         col_type(df_param/4, pos, int),
         col_type(df_param/4, param_end, int),
         col_type(flow_edge/2, from, text),
         col_type(flow_edge/2, to, text) ],
       [ (flow_edge(Arg, Param) <-
            (df_arg(CallerPath, CallStart, CallEnd, Pos, Arg, _),
             resolved_call_edge(CallerPath, CallStart, CallEnd, CalleePath, _),
             df_param(CalleePath, Param, Pos, _))) ]),
  [],
  [ [ +df_arg('src/main.rs', 10, 15, 0, secret, 16),
      +df_arg('src/main.rs', 10, 15, -1, receiver, 17),
      +df_arg('src/main.rs', 30, 35, 0, benign, 36),
      +resolved_call_edge('src/main.rs', 10, 15, 'src/f.rs', f),
      +resolved_call_edge('src/main.rs', 30, 35, 'src/g.rs', g),
      +df_param('src/f.rs', f_param, 0, 8),
      +df_param('src/g.rs', g_param, 0, 8) ] ],
  [ final(flow_edge/2, [ flow_edge(benign, g_param), flow_edge(secret, f_param) ]) ]).

% The owner span is flat in the sig record. It joins the resolved callee through
% the type callable's exact owner span, then the df parameter's slot binds the
% node-level type. The same-name decoy has a different owner span.
fixture(flow_sig_owner_join_types_the_resolved_callee,
  prog([ col_type(sink_callee/2, path, text),
         col_type(sink_callee/2, name, text),
         col_type(type_owner/4, path, text),
         col_type(type_owner/4, name, text),
         col_type(type_owner/4, start, int),
         col_type(type_owner/4, end, int),
         col_type(sig/6, path, text),
         col_type(sig/6, owner_start, int),
         col_type(sig/6, owner_end, int),
         col_type(sig/6, slot, text),
         col_type(sig/6, pos, int),
         col_type(sig/6, ty, text),
         col_type(df_param/4, path, text),
         col_type(df_param/4, node, text),
         col_type(df_param/4, pos, int),
         col_type(df_param/4, end, int),
         col_type(flow_param_type/4, path, text),
         col_type(flow_param_type/4, name, text),
         col_type(flow_param_type/4, pos, int),
         col_type(flow_param_type/4, ty, text),
         col_type(flow_node_type/4, node, text),
         col_type(flow_node_type/4, path, text),
         col_type(flow_node_type/4, name, text),
         col_type(flow_node_type/4, ty, text) ],
       [ (flow_param_type(Path, Name, Pos, Ty) <-
            (sink_callee(Path, Name),
             type_owner(Path, Name, OwnerStart, OwnerEnd),
             sig(Path, OwnerStart, OwnerEnd, param, Pos, Ty))),
         (flow_node_type(Node, Path, Name, Ty) <-
            (sink_callee(Path, Name),
             type_owner(Path, Name, OwnerStart, OwnerEnd),
             df_param(Path, Node, Pos, _),
             sig(Path, OwnerStart, OwnerEnd, param, Pos, Ty))) ]),
  [],
  [ [ +sink_callee('src/f.rs', f),
      +type_owner('src/f.rs', f, 100, 160),
      +type_owner('src/f.rs', g, 200, 260),
      +sig('src/f.rs', 100, 160, param, 0, string),
      +sig('src/f.rs', 200, 260, param, 0, integer),
      +df_param('src/f.rs', f_param, 0, 120) ] ],
  [ final(flow_param_type/4, [ flow_param_type('src/f.rs', f, 0, string) ]),
    final(flow_node_type/4, [ flow_node_type(f_param, 'src/f.rs', f, string) ]) ]).
