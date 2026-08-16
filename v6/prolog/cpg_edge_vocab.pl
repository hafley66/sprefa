% cpg_edge_vocab.pl - Joern CPG edge/node vocabulary, a REFERENCE ENUMERATION.
% REFERENCE-ONLY: for humans and design discussion; any compile/runtime code
% consulting or importing it is a defect. Source of truth: plans/2026-08-16-cpg-spec-research.REPORT.md; anchor: plans/2026-08-16-joern-cpg-striking-distance.md; schema: https://cpg.joern.io.
% @comment-ok: this file is the single documentation site for the CPG vocabulary

% cpg_edge(Name, Semantic, JoernCite, OurInterface) - 34 rows, report 1a.
% OurInterface: existing(RelName)|planned(Card)|none; RelNames are real sprefa-extract wire rels/families (v6/sprefa-extract/src/types.rs).

cpg_edge(ast,            'parent to child in the syntax tree',                                      'Ast.scala:423',        existing(cst_child)).
cpg_edge(condition,      'control structure to the expression(s) holding its condition',            'Ast.scala:428',        planned(cfg_edge)).
cpg_edge(true_body,      'control structure to its true branch/body',                              'Ast.scala:434',        planned(cfg_edge)).
cpg_edge(false_body,     'control structure to its false branch/body',                             'Ast.scala:439',        planned(cfg_edge)).
cpg_edge(do_body,        'do-while control structure to its body',                                 'Ast.scala:445',        planned(cfg_edge)).
cpg_edge(for_init,       'for-loop to its initialization expression(s)',                           'Ast.scala:450',        planned(cfg_edge)).
cpg_edge(for_update,     'for-loop to its update/step expression(s)',                              'Ast.scala:457',        planned(cfg_edge)).
cpg_edge(for_body,       'for-loop to its body',                                                   'Ast.scala:463',        planned(cfg_edge)).
cpg_edge(try_body,       'try control structure to its try body',                                  'Ast.scala:467',        planned(cfg_edge)).
cpg_edge(catch_body,     'try control structure to its catch/handler bodies',                      'Ast.scala:472',        planned(cfg_edge)).
cpg_edge(finally_body,   'try control structure to its finally body',                              'Ast.scala:479',        planned(cfg_edge)).
cpg_edge(jump_argument,  'jump-like control structure to the node encoding its jump target',       'Ast.scala:486',        planned(cfg_edge)).
cpg_edge(cfg,            'control flow from source to destination node',                           'Cfg.scala:55',         planned(cfg_edge)).
cpg_edge(dominate,       'source node immediately dominates the destination',                      'Dominators.scala:26',  planned(cdg_edge)).
cpg_edge(post_dominate,  'source node immediately post-dominates the destination',                 'Dominators.scala:33',  planned(cdg_edge)).
cpg_edge(cdg,            'destination is control dependent on the source',                         'Pdg.scala:38',         planned(cdg_edge)).
cpg_edge(reaching_def,   'a variable produced at the source reaches the destination without being reassigned on the way (VARIABLE property names it)', 'Pdg.scala:43', existing(df_direct)).
cpg_edge(call,           'call site (a CALL node) to the METHOD it invokes; optional, auto-created from METHOD_FULL_NAME on load', 'CallGraph.scala:142', existing(call)).
cpg_edge(argument,       'call site to its arguments, and RETURN to the expressions it returns',   'CallGraph.scala:155',  existing(df_arg)).
cpg_edge(receiver,       'call site to its receiver argument (the object assigned to `this`)',     'CallGraph.scala:165',  existing(df_arg)).
cpg_edge(ref,            'source identifier denotes access to the destination node (e.g. identifier to a local)', 'Base.scala:162', existing(scip_ref)).
cpg_edge(eval_type,      'a node to its evaluation type',                                          'Shortcuts.scala:44',   existing(type)).
cpg_edge(contains,       'a node to the method that contains it',                                  'Shortcuts.scala:48',   existing(cst_child)).
cpg_edge(parameter_link, 'method input parameter to the corresponding output parameter',           'Shortcuts.scala:52',   none).
cpg_edge(capture,        'capturing of a variable into a closure',                                 'Hidden.scala:72',      none).
cpg_edge(captured_by,    'a captured LOCAL to the corresponding CLOSURE_BINDING',                  'Hidden.scala:88',      none).
cpg_edge(imports,        'imports to dependencies',                                                'Hidden.scala:266',     existing(specifier)).
cpg_edge(is_call_for_import, 'a CALL in the AST to the IMPORT',                                    'Hidden.scala:270',     none).
cpg_edge(tagged_by,      'nodes to the tags they are tagged by',                                   'Tags.scala:48',        none).
cpg_edge(binds,          'a TYPE_DECL with a BINDING node',                                        'Binding.scala:55',     none).
cpg_edge(binds_to,       'type arguments to type parameters',                                      'Type.scala:165',       existing(type_generic)).
cpg_edge(alias_of,       'alias relation between a type declaration and a type',                   'Type.scala:174',       none).
cpg_edge(inherits_from,  'inheritance between a type declaration and a type',                      'Type.scala:184',       none).
cpg_edge(source_file,    'a node to the node representing its source file',                        'FileSystem.scala:84',  none).

% Node kinds (report section 1b), statements and expressions.
cpg_node(method,           'a procedure/function/method; the ENTRY node of the CFG', 'Method.scala:45').
cpg_node(method_return,    'the formal return parameter; the EXIT node of the CFG', 'Method.scala:116').
cpg_node(call,             'a (function/method/procedure) call, METHOD_FULL_NAME names the target', 'Ast.scala:734').
cpg_node(identifier,       'an identifier referring to a variable by name',         'Ast.scala:155').
cpg_node(literal,          'a literal such as an integer or string constant',       'Ast.scala:128').
cpg_node(control_structure,'a control structure or conditional/unconditional jump; its CONTROL_STRUCTURE_TYPE is one of BREAK/CONTINUE/DO/WHILE/FOR/GOTO/IF/ELSE/TRY/THROW/SWITCH/MATCH/YIELD/CATCH/FINALLY', 'Ast.scala:393').
cpg_node(block,            'a compound statement',                                   'Ast.scala:111').
cpg_node(local,            'a local variable',                                       'Ast.scala:141').
cpg_node(field_identifier, 'the field accessed in a field access',                   'Ast.scala:183').
cpg_node(return,           'a return instruction (`return x`), distinct from METHOD_RETURN', 'Ast.scala:316').
cpg_node(jump_target,      'a location specifically marked as a jump target',        'Ast.scala:273').
cpg_node(method_ref,       'a reference to a method as it appears in an expression', 'Ast.scala:299').
cpg_node(type_ref,         'a reference to a type/class',                            'Ast.scala:310').
cpg_node(modifier,         'a language-dependent modifier (static, private, ...)',   'Ast.scala:261').
cpg_node(unknown,          'catch-all for AST nodes with no suitable CPG kind',      'Ast.scala:411').
cpg_node(file,             'a source file; AST root',                                'FileSystem.scala:95').
cpg_node(namespace,        'a namespace',                                            'Namespace.scala:50').
cpg_node(type,             'a type instance',                                        'Type.scala:153').
cpg_node(type_decl,        'a type declaration',                                     'Type.scala:78').
cpg_node(member,           'a type member',                                          'Type.scala:139').

% go/0: gate. Assert exactly 34 edge rows, every cite non-empty and matching
% \w+\.scala:\d+, every OurInterface one of the three shapes, node rows nonzero.
go :-
    findall(E, cpg_edge(E, _, _, _), EdgeNames),
    length(EdgeNames, EdgeCount),
    (   EdgeCount =:= 34
    ->  forall(cpg_edge(_, _, Cite, Iface),
               (   valid_cite(Cite),
                   interface_shape(Iface)))
    ;   true
    ),
    findall(N, cpg_node(N, _, _), NodeNames),
    length(NodeNames, NodeCount),
    (   EdgeCount =:= 34,
        forall(cpg_edge(_, _, Cite, Iface),
               (   valid_cite(Cite),
                   interface_shape(Iface))),
        NodeCount > 0
    ->  format('PASS: ~d cpg_edge rows, ~d cpg_node rows~n', [EdgeCount, NodeCount]),
        halt(0)
    ;   format('FAIL: cpg_edge=~d cpg_node=~d~n', [EdgeCount, NodeCount]),
        halt(1)
    ).

interface_shape(existing(_)).
interface_shape(planned(_)).
interface_shape(none).

valid_cite(Cite) :-
    atom_codes(Cite, Codes),
    append(FileCodes, [46|Tail], Codes),
    file_codes_ok(FileCodes),
    append([115, 99, 97, 108, 97], [58|NumCodes], Tail),
    NumCodes = [_|_],
    maplist(digit_codes, NumCodes).

file_codes_ok([C|Cs]) :-
    name_char_codes(C),
    maplist(name_char_codes, Cs).

name_char_codes(C) :-
    char_code(Ch, C),
    char_type(Ch, alnum) ; C =:= 0'_ .

digit_codes(C) :-
    char_code(Ch, C),
    char_type(Ch, digit).
