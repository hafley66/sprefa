% registry.pl: the compiler's surface construct inventory.
%
% surface(Functor/Arity, Axis, AnalyzeRole, LowerRole, Status)
%
% Arity is an integer except for combine/variadic. Status is live, reserved,
% or refused. LowerRole carries both the textual body shape and the gate role
% so the parser, printer, analyzer, and supported-subset gate can project from
% these rows without maintaining their own functor lists.

% expression/5 is a SECOND, SEPARATE table on a different axis, added by rank 5
% of plans/2026-07-29-prolog-org-review.md. surface/5 says which functors are
% body syntax and what the analyzer and the gate do with them; expression/5
% says how the arithmetic and comparison operators PRINT, LOWER, and TYPE.
% Keeping them apart is deliberate: widening surface/5 with precedence and SQL
% text would put rendering metadata on rows like not/1 and match/2 that have
% no rendering. The two tables overlap on the eleven operator functors and
% expression_table_agrees_with_surface_rows asserts they stay consistent.
:- module(registry,
          [ surface/5,
            surface_for_term/6,
            body_surface_for_term/6,
            wrapper_lower_role/3,
            bind_definition/2,
            bind_executor/2,
            expression/5,
            expression_for_term/5,
            cli_command/3
          ]).

% Contextual gate: live around one plain relation atom in an edge body;
% analyze.pl retains latest_in_level_rule and edge_body_with_latest for the
% wider placements.
surface(latest/1,       sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom, lower),              live).
% Contextual gate like latest/1: live around one plain relation atom in an
% EDGE body, where it is the departure trigger (the arm reads the rel's
% departure frontier -- last tick's net -delta rows). analyze.pl retains
% finalize_in_level_rule and edge_body_with_finalize for the wider placements.
surface(finalize/1,     time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, lower),              live).
surface(next/1,         time,      splice_bare,                  wrapper(rel_atom, lower),              live).
surface(combine/variadic, join,    splice_bare,                  wrapper(atom_list, lower),             live).
surface(zip/2,          join,      splice_bare,                  wrapper(atom_list, refuse(functor)),   reserved).

surface(unsubscribe/1,  time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(complete/1,     time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(subscribe/1,    time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(error/1,        time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).

surface(not/1,          sign,      arm(neg),                     wrapper(body_item, lower),             live).
surface(pre/1,          sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom, refuse(goal)),       refused).
% Contextual gate, same shape as latest/1: live around a plain VARIABLE in an
% edge body (lowered to a read of the emitted __tick counter); analyze.pl
% keeps now_in_level_rule and edge_body_with_now for the wider placements.
surface(now/1,          time,      no_refs,                      wrapper(expr, lower),                  live).
% STRUCT-AS-ROWS (SLOT-DECODE-SURFACE): decode/2 stays on the surface as sugar
% and lowers to a dictionary JOIN in a LEVEL body over a struct-typed column
% (lower.pl expand_decode_rules/4). Every wider placement keeps a named
% refusal: an edge body is edge_body_needs_json_destructure (the untyped
% compound-arrival encoding, SLOT-TERM-STRUCT), an untyped source is
% decode_source_not_struct, a non-object pattern is decode_pattern_not_object,
% and a key the type does not declare is decode_field_unknown.
surface(decode/2,       guard,     no_refs,                      wrapper(expr_pair, lower),             live).
surface(json_each/2,    guard,     no_refs,                      wrapper(expr_pair, refuse(goal)),      refused).
surface(true/0,         guard,     no_refs,                      word(lower),                           live).

% EXPRESSION + AGGREGATE LIFT (ruling expression_residency,
% fuse_to_sql_deltas_ts_deopt_last): binds, comparisons and the decomposable
% aggregate heads lower into the emitted SQL rather than being refused. The
% LowerRole gate word is what analyze.pl's supported-subset checks read, so
% flipping refuse(...) to lower(...) here is what un-refuses them; the
% textual shape word (infix/head) is unchanged, so parse_dl.pl and
% print_dl.pl project exactly as before.
surface(':='/2,         bind,      no_refs,                      infix(lower),                          live).
surface(is/2,           bind,      no_refs,                      infix(lower),                          live).

surface('<'/2,          guard,     no_refs,                      infix(lower),                          live).
surface('=<'/2,         guard,     no_refs,                      infix(lower),                          live).
surface('>'/2,          guard,     no_refs,                      infix(lower),                          live).
surface('>='/2,         guard,     no_refs,                      infix(lower),                          live).
surface('=='/2,         guard,     no_refs,                      infix(lower),                          live).
surface('\\=='/2,       guard,     no_refs,                      infix(lower),                          live).

surface(count/1,        aggregate, no_refs,                      head(lower),                           live).
surface(sum/1,          aggregate, no_refs,                      head(lower),                           live).
surface(min/1,          aggregate, no_refs,                      head(lower),                           live).
surface(max/1,          aggregate, no_refs,                      head(lower),                           live).
% json_array/json_object stay REFUSED per ruling json_ticklog_encoding:
% the oracle tick-log and final-state encoders now emit canonical JSON text
% for JSON values, with sorted object keys and no whitespace, while plain
% compounds keep canonical term text. The compiler-side aggregate heads
% remain refused until a later arc supplies matching json_group_array/
% json_group_object lowering and byte-identity coverage. The refusal itself
% does not depend on the old cons-term rendering argument.
surface(json_array/1,   aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(json_object/2,  aggregate, no_refs,                      head(refuse(aggregate)),               refused).

surface(enum_decl/2,     decl,      no_refs,                      decl(enum_variants),                    live).
surface(';' /2,          decl,      no_refs,                      decl(enum_variant_separator),           live).
surface(col_type/3,      decl,      no_refs,                      decl(column_type),                      live).
% STRUCT-AS-ROWS (ruling compound_storage = struct_as_rows). A declared
% struct type. Its values live in a storage-plane dictionary keyed on
% canonical content; a column typed with the name stores the ref. The
% dictionary is deliberately NOT a rel: it never appears in relColumns, in a
% boundary read or in the tick log (arc header Edge 2).
surface(type_decl/2,     decl,      no_refs,                      decl(struct_type),                      live).
surface(set/0,           decl,      no_refs,                      decl(refuse(removed_word)),            refused).
surface(match/2,         sugar,     no_refs,                      block(match_arms),                      live).
surface(sh_decl/4,       world,     no_refs,                      decl(host_plan),                        live).
surface(probe/4,         world,     no_refs,                      wrapper(host_probe, lower),             live).
surface(bind_decl/2,     world,     no_refs,                      decl(bind_plan),                        live).
surface(query/1,         read,      no_refs,                      decl(query_plan),                       live).
surface(ts_query/1,      world,     no_refs,                      value(tree_sitter_query),               live).
surface(sg_pattern/3,    world,     no_refs,                      value(refuse(slot_sg_metavariable_semantics)), refused).

% ═══ expression operators ═══════════════════════════════════════════════════
%
% expression(Operator/Arity, Family, PrintPrecedence, SqlRendering, TypeRule)
%
%   Family          arithmetic | ordered_comparison | identity_comparison
%   PrintPrecedence binding tightness for print_dl.pl's parenthesizer. Only
%                   arithmetic is printed by that path, so comparisons carry 0.
%   SqlRendering    infix(SqlOperator), or a named template where SQLite's
%                   operator does not match this language's semantics.
%   TypeRule        both_int  operands must both be int (the Int-only law)
%                   same_type operands must agree, whatever the type
%                   text_only operand must be text
%
% Before this table, the same eleven operators were listed in five places:
% body.pl's comparison_goal/1, lower.pl's arithmetic_expr/4 and
% comparison_operator_sql/5, print_dl.pl's arith_op/2, and two memberchk lists
% in analyze.pl. Adding an operator meant finding all five.
%
% mod is the row that shows why SqlRendering is not just an operator atom:
% SQLite's % takes the sign of the dividend, while this language's mod follows
% the divisor, so it renders as a sign-corrected template.

expression('+'/2,    arithmetic,          1, infix('+'),             both_int).
expression('-'/2,    arithmetic,          1, infix('-'),             both_int).
expression('*'/2,    arithmetic,          2, infix('*'),             both_int).
expression('/'/2,    arithmetic,          2, infix('/'),             both_int).
expression(mod/2,    arithmetic,          2, sign_corrected_modulo,  both_int).

expression('<'/2,    ordered_comparison,  0, infix('<'),             both_int).
expression('=<'/2,   ordered_comparison,  0, infix('<='),            both_int).
expression('>'/2,    ordered_comparison,  0, infix('>'),             both_int).
expression('>='/2,   ordered_comparison,  0, infix('>='),            both_int).

expression('=='/2,   identity_comparison, 0, infix('='),             same_type).
expression('\\=='/2, identity_comparison, 0, infix('<>'),            same_type).

% V5 `sprf_norm`: retain ASCII letters/digits and lowercase letters. This is
% an existing expression-call shape; lowering stays inside SQLite.
expression(norm/1,    text_scalar,         3, ascii_alnum_lower,     text_only).

expression_for_term(Term, Family, Precedence, SqlRendering, TypeRule) :-
    nonvar(Term),
    functor(Term, Name, Arity),
    expression(Name/Arity, Family, Precedence, SqlRendering, TypeRule).

% ═══ world push sources (bind_decl) ═════════════════════════════════════════
%
% bind_definition(Name, Columns)  the row shape the served runtime pushes
% bind_executor(Name, Executor)   the executor name emitted into the plan
%
% COLUMN 1 IS THE CONFIGURATION COLUMN, for every bind: the program's own rules
% state which cadences / which file sets they consume as LITERALS in that
% position (`interval(2, Bucket)`, `watch("src/**/*.ts", Path, Digest)`), and
% emit_ts.pl's bind_read_literals/4 collects exactly those. A declared bind
% whose rules read no literal gets an empty list and therefore no live source
% at all -- an honest zero, never an invented default.
%
% `watch` is the file-watcher push bind (golden plan phase 2). Its row is
% (glob, path, digest): the DIGEST is what makes it a freshness source rather
% than a notification -- a save that does not change content re-emits an
% identical row, which is zero delta at the rel boundary, so nothing
% downstream re-derives (ruling salt_minting = content_addressed; the digest
% IS the salt every demand host addresses its cache by). Presence and absence
% ride the ARRIVAL SIGN, not a second column and not a null: a removed file is
% a `-` arrival of the row that was there. See v6/tsv2/serve/2_binds.ts.
bind_definition(interval, [col(period, int), col(bucket, int)]).
bind_definition(watch,    [col(glob, text), col(path, text), col(digest, text)]).

bind_executor(interval, live_interval).
bind_executor(watch,    live_watch).

surface_for_term(Term, Functor/Arity, Axis, AnalyzeRole, LowerRole, Status) :-
    nonvar(Term),
    functor(Term, Functor, Arity),
    ( surface(Functor/Arity, Axis, AnalyzeRole, LowerRole, Status)
    ; surface(Functor/variadic, Axis, AnalyzeRole, LowerRole, Status)
    ).

body_surface_for_term(Term, Signature, Axis, AnalyzeRole, LowerRole, Status) :-
    surface_for_term(Term, Signature, Axis, AnalyzeRole, LowerRole, Status),
    body_lower_role(LowerRole).

body_lower_role(wrapper(_, _)).
body_lower_role(word(_)).
body_lower_role(infix(_)).

wrapper_lower_role(wrapper(Shape, GateRole), Shape, GateRole).

% ═══ CLI ("the bop") command table ═══════════════════════════════════════════
%
% cli_command(Verb, ArgSpec, Summary)
%
% The single source of the verb inventory (user directive 2026-07-29 late:
% "registry.pl grows a cli command table"). ArgSpec mirrors how the row reads
% as a usage line, not a formal grammar -- <required>, [--flag <value>] for an
% option that takes a value, [--flag] for a boolean switch. The TS side
% (v6/tsv2/cli/bop.ts) wires the identical five verbs through commander;
% tests/bopCommandInventory.test.ts asserts the two inventories agree on verb
% NAMES by grepping this file's cli_command/3 rows and bop.ts's own
% `.command(...)` lines rather than sharing a generated manifest file -- the
% simpler of the two mechanisms the brief offered (JSON manifest export vs a
% grep-style cross-check), chosen because the TS side has no consult-time
% access to this table and a manifest step would be one more artifact to keep
% in sync for five rows that rename rarely.
%
% run/check boot the served tsv2 engine IN-PROCESS (server-calls-itself,
% no daemon concept -- the same user directive). serve is the long-running
% entry; run and check are one-shot processes that start their own server,
% use it, and exit.
cli_command(serve, '[--port <port>] [--db <url>]',
            'boot the served tsv2 engine and keep it running (exactly serve/main.ts).').
cli_command(run,   '<file.dl6> [--ticks <n>] [--port <port>]',
            'compile + load a program on an in-process ephemeral server, stream ticks to stdout until quiescent or --ticks fires, then shut down cleanly.').
cli_command(check, '<file.dl6>',
            'validate a program through the text door; no server boots. Exit 0 clean, 2 named-refusal findings, 1 broken (parse/compile error).').
cli_command(load,  '<file.dl6> [--port <port>]',
            'POST a compiled program to an already-running bop serve; exit 1 if nothing is listening.').
cli_command(q,     '<rel> [--port <port>] [--json]',
            'read one rel''s current rows from a running bop serve.').
