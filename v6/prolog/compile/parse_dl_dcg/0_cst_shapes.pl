cst_shape(decl_a_column/1,  declaration_parameter-[name, type]).
cst_shape(enum_variants/1,  enum_variants-[]).
cst_shape(rel_modifiers/2,  repeat(relation_modifier)-[]).
cst_shape(match_stmt/1,     match_statement-[scrutinee]).
cst_shape(match_arm/1,      match_arm-[guard, arrow, head]).
cst_shape(braces_term/1,    object_pattern-[]).
cst_shape(dotted_path/1,    path-[]).
cst_shape(statement/2,      statement-[]).
cst_shape(statements/3,     source_file-[]).
cst_shape(rel_stmt/1,       relation_declaration-[]).
cst_shape(interface_stmt/1, interface_declaration-[]).
cst_shape(typed_col/2,      ref(column)-[]).
cst_shape(type_expr/1,      type-[]).
cst_shape(annotation_type/1, type_annotation-[type, applications]).
cst_shape(annotation_application/1, annotation_application-[name]).
cst_shape(annotation_list/1, annotation_list-[]).
cst_shape(annotation_argument/1-named, annotation_named_argument-[name, value]).
cst_shape(enum_variant/1,   ref(enum_variant)-[]).
cst_shape(rule_stmt/1,      rule-[head, arrow, body]).
cst_shape(query_stmt/1,     ref(query)-[]).
cst_shape(body/1,           ref(goal_list)-[]).
cst_shape(expr/1,           ref(expression)-[]).
cst_shape(head_atom/1,      atom-[name]).
cst_shape(brace_pair/1,     object_pair-[key, value, type]).
cst_shape(json_object/1,    json_object-[]).
cst_shape(json_array/1,     json_array-[]).
cst_shape(json_pair/1,      json_pair-[key, value]).
cst_shape(list_term/1,      list-[]).
cst_shape(int_lit/1,        ref(integer)-[]).
cst_shape(float_lit/1,      ref(float)-[]).
cst_shape(string_lit/1,     string-[]).
cst_shape(atom_lit/1,       quoted_atom-[]).
cst_shape(template_lit/1,   template-[]).
cst_shape(bool_lit/1,       boolean-[]).
cst_shape(ident/1,          ref(identifier)-[]).
% editor nodes the parser folds with no named nonterminal; the emitter
% renders each from its fixed editor shape (editor_* keys are not parser preds)
cst_shape(editor_paren/1,   parenthesized_expression-[]).
cst_shape(editor_literal/1, literal-[]).
cst_shape(editor_member/1,  member_expression-[]).

% Nodes the parser folds away: Nonterminal-Marker -> Node, inner = the
% marked branch alone rather than the nonterminal with that branch chosen.
cst_origin(atom_arg/1-named,    named_argument-[name, value]).
cst_origin(rule_stmt/1-true,    fact-[]).
cst_origin(brace_key/1-'$',     capture_key-[]).
cst_origin(dot_chain/2-dot_get, member_access-[]).
cst_origin(list_term/1-spread,  inner(spread_element)-[]).

unsupported(Surface) :- assertz(finding_fact(unsupported_surface(Surface))).
record_cols(Name, Cols) :-
    retractall(rel_column_order_fact(Name, _)),
    assertz(rel_column_order_fact(Name, Cols)).
lookup_column_order(Name, Cols) :- rel_column_order_fact(Name, Cols).
record_host_signature(Name, Ins, Outs) :-
    retractall(host_signature_fact(Name, _, _)),
    assertz(host_signature_fact(Name, Ins, Outs)).
record_host_path(Name, Segments) :-
    retractall(host_path_fact(Name, _)),
    assertz(host_path_fact(Name, Segments)).


