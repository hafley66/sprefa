% registry.pl: the compiler's surface construct inventory.
%
% surface(Functor/Arity, Axis, AnalyzeRole, LowerRole, Status)
%
% Arity is an integer except for combine/variadic. Status is live, reserved,
% or refused. LowerRole carries both the textual body shape and the gate role
% so the parser, printer, analyzer, and supported-subset gate can project from
% these rows without maintaining their own functor lists.

% expression/5 stores print, lower, and type metadata separately from
% surface/5. The overlapping operator rows are checked for consistency.
:- module(registry,
          [ surface/5,
            surface_for_term/6,
            body_surface_for_term/6,
            wrapper_lower_role/3,
            bind_definition/2,
            bind_executor/2,
            host_executor/2,
            host_execution/3,
            host_executor_contract/2,
            host_input_contract/3,
            host_input_roles/3,
            http_route/3,
            expression/5,
            expression_for_term/5,
            cli_command/3,
            clock_role/4,
            trace_event/2
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
% RULING null_design = get_else_use_site_never_storage (conformance/rulings.pl,
% ARCH get_else_wiring). `coalesce(rel_atom(Bound..., Out), Default)` is the
% total read: Out binds from the row when one exists, from Default when none
% does, and the tuple survives either way. Null never reaches storage or the
% type system; absence stays row absence and the consumer that wants totality
% spells the default itself. Datomic `get-else` is the prior art; the WORD is
% `coalesce` because the vocabulary law admits only rxjs/prolog/SQL words and
% COALESCE is SQL's name for exactly this.
%
% SUGAR axis, and it means it: 0_coalesce_expand.pl (expansion phase 45, the
% one module BOTH doors consult) rewrites every coalesce into two ordinary
% clauses -- the bare/latest read, and not(...) plus a `:=` of the default --
% before analyze.pl or engine.pl ever sees a program. The gate word is
% `expand(coalesce)` rather than `lower` because nothing in lower.pl handles
% this functor and nothing should: a coalesce reaching the lowering would be a
% phase-order defect, and the expander's own coalesce_not_top_level unsupported construct is
% what makes that unreachable. The AnalyzeRole is stated honestly all the same,
% so the pre-expansion readers (print_dl.pl's decl synthesis) see the source
% relation as the sampled reference it is.
surface(coalesce/2,     sugar,     refs_of_arg(1, pos, sampled), wrapper(rel_atom_default, expand(coalesce)), live).
surface(pre/1,          sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom, lower),              live).
surface(pre/2,          sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom_default, lower),      live).
surface(seq/1,          sugar,     no_refs,                      wrapper(expr, expand(seq)),           live).
% Contextual gate, same shape as latest/1: live around a plain VARIABLE in an
% edge body (lowered to a read of the emitted __tick counter); analyze.pl
% keeps now_in_level_rule and edge_body_with_now for the wider placements.
surface(now/1,          time,      no_refs,                      wrapper(expr, lower),                  live).
% STRUCT-AS-ROWS (SLOT-DECODE-SURFACE): decode/2 stays on the surface as sugar
% and lowers to a dictionary JOIN in a LEVEL body over a struct-typed column
% (lower.pl expand_decode_rules/4). Every wider placement keeps a named
% unsupported construct: an edge body is edge_body_needs_json_destructure (the untyped
% compound-arrival encoding, SLOT-TERM-STRUCT), an untyped source is
% decode_source_not_struct, a non-object pattern is decode_pattern_not_object,
% and a key the type does not declare is decode_field_unknown.
surface(decode/2,       guard,     no_refs,                      wrapper(expr_pair, lower),             live).
surface(json_each/2,    guard,     no_refs,                      wrapper(expr_pair, refuse(goal)),      refused).

% ═══ the json value/pattern axis ════════════════════════════════════════════
%
% These are NOT body goals: every row below is a shape that appears inside a
% VALUE or inside decode/2's second argument, which is why each carries a
% `value(...)` lowering role and none of them reaches body_lower_role/1. They
% are registry rows all the same, because the registry is what the generated
% SYNTAX.md table and the golden-flex coverage gate read -- a surface that
% grows without a row here grows in silence.
%
% Their TEXT surface is punctuation, not a word, so registry_word_regex/2 in
% 1_emit_registry_docs.pl excludes the whole axis from the tmLanguage keyword
% alternation; highlighting `spread` as a keyword would paint any relation a
% user happens to call `spread`, and the text spelling is `[... p]` anyway.
%
%   term          text         ruling
%   '{}'/1        {k: v}       json5_subset = unquoted_keys_only
%   '{}'/0        {}           the empty object; arity 0 because that is what
%                              the term door's own reader produces
%   spread/1      [... p]      the flagship's array fan-out
%   '$'/1         $name        json_key_hole_marker = dollar
%   '**'/0        **           descent_depth_cap = uncapped
%
% THE TYPED CAPTURE `{stars: Stars: int}` is a production of `'{}'/1` above
% and deliberately has NO row of its own. Its term is `:`/2 -- the colon is
% already this language's type marker (ruling decl_column_spelling =
% colon_typed_ordered_columns) and `:` is 600 xfy in SWI, so
% `stars: Stars: int` reads as `:(stars, :(Stars, int))` with no term-door
% work. `:`/2 is ALSO the ordinary key/value separator inside every braces
% literal, which is exactly why a row would be wrong: the coverage gate reads
% a row's signature out of the parsed term, and `:`/2 occurs in every brace
% pair the golden already writes, so the signature can neither be satisfied
% by a typed capture nor honestly excused. The pair separator itself has no
% row for the same reason.
%
% Live capture types are `int` / `float` / `text`, one per json1 `json_type`
% answer, checked identically at both doors (body.pl json_capture_type/2,
% lower.pl json_capture_json_type/2). Anything else, `bool` included, is the
% named unsupported construct json_capture_type_unknown -- `bool` because json_flex card C4
% measured a top-level json `true` degrading to the integer 1 through the real
% emitted arrival statement, so its storage is an open card rather than a
% settled type.
surface('{}'/1,         json,      no_refs,                      value(json_object_shape),               live).
surface('{}'/0,         json,      no_refs,                      value(json_empty_object),               live).
surface(spread/1,       json,      no_refs,                      value(json_array_spread),               live).
surface('$'/1,          json,      no_refs,                      value(json_hole),                       live).
surface('**'/0,         json,      no_refs,                      value(json_descent),                    live).
% CARD-BRACE-TAG, settled by measurement: `_{...}` and `Tag{...}` are SWI DICT
% syntax, a term shape `{}`/1 can never unify with, so the term door could
% never agree with a text door that read them as json. Reserved rather than
% silently misparsed (parse_dl.pl refuse_tagged_brace/1), and reserved on
% purpose: it is the home for the directive's stated future use of `{`.
surface(tagged_brace/1, json,      no_refs,                      value(refuse(tagged_brace_reserved)),   reserved).
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
surface('=:='/2,         guard,     no_refs,                      infix(lower),                          live).
surface('=\\='/2,         guard,     no_refs,                      infix(lower),                          live).
surface(regexp/2,        guard,     no_refs,                      wrapper(expr_pair, lower),             live).

surface(count/1,        aggregate, no_refs,                      head(lower),                           live).
surface(sum/1,          aggregate, no_refs,                      head(lower),                           live).
surface(min/1,          aggregate, no_refs,                      head(lower),                           live).
surface(max/1,          aggregate, no_refs,                      head(lower),                           live).
surface(avg/1,          aggregate, no_refs,                      head(lower),                           live).
% The compiler and oracle share the aggregate names below. JSON values use
% the canonical JSON text boundary, so the SQL aggregate result and the tick
% log carry the same bytes.
surface(json_array/1,       aggregate, no_refs,                   head(refuse(aggregate)),               refused).
surface(json_object/2,      aggregate, no_refs,                   head(lower),                           live).
surface(json_group_array/1, aggregate, no_refs,                   head(lower),                           live).
surface(json_group_array/2, aggregate, no_refs,                   head(lower),                           live).
surface(group_concat/2,     aggregate, no_refs,                   head(lower),                           live).
surface(group_concat/3,     aggregate, no_refs,                   head(lower),                           live).
% The one-argument spelling defaults its separator to SQLite's own `,`
% (group_concat/1 and group_concat/2 with `,` are byte-identical).
surface(group_concat/1,     aggregate, no_refs,                   head(lower),                           live).

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
% scan is reserved at every arity. The supported file-enumeration spellings
% are files and files_at.
surface(scan/variadic,   world,     no_refs,                      goal(refuse(removed_word)),            reserved).
surface(match/2,         sugar,     no_refs,                      block(match_arms),                      live).
surface(sh_decl/4,       world,     no_refs,                      decl(host_plan),                        live).
surface(probe/4,         world,     no_refs,                      wrapper(host_probe, lower),             live).
surface(bind_decl/2,     world,     no_refs,                      decl(bind_plan),                        live).
surface(query/1,         read,      no_refs,                      decl(query_plan),                       live).
surface(ts_query/1,      world,     no_refs,                      value(tree_sitter_query),               live).
surface(sg_pattern/3,    world,     no_refs,                      value(refuse(slot_sg_metavariable_semantics)), refused).

% ═══ clock dependency roles ════════════════════════════════════════════════
%
% These rows classify the existing rule-body roles. They add no surface
% construct and no second type vocabulary. `3_clock_check.pl` projects them
% onto ordinary relation dependencies as:
%
%   clock_dependency(Rule, From, To, ReadRing, WriteRing, Sign, Grade)
%
% `source_delay` is resolved from the program graph: an outside or level
% boundary can fire an edge arm in the current tick, while an occurrence
% written by another edge arm is carried to the next tick.
clock_role(level_read,       b, positive, 0).
clock_role(level_absence,    b, negative, 0).
clock_role(edge_trigger,     z, positive, source_delay).
clock_role(edge_departure,   z, negative, 1).
clock_role(edge_sample,      b, state,    0).
clock_role(edge_pre,         b, previous, -1).
clock_role(edge_absence,     b, negative, 0).

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
% mod is the row that shows why SqlRendering is not just an operator atom:
% SQLite's % takes the sign of the dividend, while this language's mod follows
% the divisor, so it renders as a sign-corrected template.

expression('+'/2,    arithmetic,          1, infix('+'),             both_number).
expression('-'/2,    arithmetic,          1, infix('-'),             both_number).
expression('*'/2,    arithmetic,          2, infix('*'),             both_number).
expression('/'/2,    arithmetic,          2, numeric_division,       both_number).
expression(mod/2,    arithmetic,          2, sign_corrected_modulo,  both_int).

expression('<'/2,    ordered_comparison,  0, infix('<'),             both_number).
expression('=<'/2,   ordered_comparison,  0, infix('<='),            both_number).
expression('>'/2,    ordered_comparison,  0, infix('>'),             both_number).
expression('>='/2,   ordered_comparison,  0, infix('>='),            both_number).

expression('=='/2,   identity_comparison, 0, infix('IS'),             same_type).
expression('\\=='/2, identity_comparison, 0, infix('IS NOT'),         same_type).

% Numeric (arithmetically-evaluated) equality pair; an operator with no
% expression/5 row refused by name (unknown_comparison_operator).
expression('=:='/2,   ordered_comparison, 0, infix('='),             both_number).
expression('=\\='/2,   ordered_comparison, 0, infix('<>'),            both_number).

% V5 `sprf_norm`: retain ASCII letters/digits and lowercase letters. This is
% an existing expression-call shape; lowering stays inside SQLite.
expression(norm/1,    text_scalar,         3, ascii_alnum_lower,     text_only).
% The str-stratum minimum: rtrim/2 and replace/3 are SQLite scalar functions
% with no registered UDF, so the rendering is the bare SQLite call. This is
% the shape that lets a program derive a directory prefix with
% rtrim(path, replace(path, '/', '')).

% The str-stratum minimum plus the pure-text string scalars: every row is
% all-text-operand, so the Rendering equals the SQLite scalar name.
expression(upper/1,   text_scalar,         3, upper,                text_only).
expression(lower/1,   text_scalar,         3, lower,                text_only).
expression(trim/1,    text_scalar,         3, trim,                 text_only).
expression(trim/2,    text_scalar,         3, trim,                 text_only).
expression(ltrim/1,   text_scalar,         3, ltrim,                text_only).
expression(ltrim/2,   text_scalar,         3, ltrim,                text_only).
expression(rtrim/1,   text_scalar,         3, rtrim,                text_only).
expression(rtrim/2,   text_scalar,         3, rtrim,                text_only).
expression(reverse/1, text_scalar,         3, reverse,              text_only).
expression(replace/3, text_scalar,         3, replace,              text_only).
% SQL INITCAP (Oracle/Postgres) semantics: word boundary = any non-alnum,
% rest lowercased; SQLite lacks it, so it renders as a norm-style CTE.
expression(initcap/1, text_scalar,         3, initcap_words,        text_only).

% The typed string scalars: substr/instr/length mix text and int operands and
% return mixed results, so text_only (all-text operands) and the hardwired text
% result cannot carry them. The TypeRule is typed(OperandTypes, ResultType),
% one entry per operand in argument order; the compile sites lower each operand
% by its declared type and set the result type from the row.
expression(substr/2,  typed_scalar, 3, substr, typed([text, int],      text)).
expression(substr/3,  typed_scalar, 3, substr, typed([text, int, int], text)).
expression(instr/2,   typed_scalar, 3, instr,  typed([text, text],     int)).
expression(length/1,  typed_scalar, 3, length, typed([text],           int)).
% split answers the json array carrier `decode(Parts, [... Part])` already fans
% out; SQLite has no split scalar, so the rendering is not the function name.
expression(split/2,   typed_scalar, 3, split_json_array, typed([text, text], json)).
% RFC 7396 merge patch, the streaming scan operator over two json documents.
% Its own family because text_only would reject a json operand.
expression(json_patch/2, json_scalar,      3, json_patch,           json_only).

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

% `sh` remains the only host declaration surface. The emitted plan holds this
% target-neutral key. `extract` is the existing extraction declaration name;
% its input is fixed, while its output stays the declaration's JSONL projection.
host_executor(extract, sprefa_extract) :- !.
host_executor(_,       shell).

% Execution is selected from the declaration as a whole. The old name-based
% `extract` row remains for compatibility, while every fixed
% `$DL_EXTRACT_BIN ... {path}` declaration uses the same target-neutral
% executor regardless of which named projection receives its stdout.
%
% REPO-SCOPED EXTRACTION (ruling repo_column_spelling = distinct_name_hosts,
% applied one level down). A crawl runs the extractor against files in ANOTHER
% working tree, so its template reads `{repo}/{path}` and its input list is
% three columns wide. That is a different contract, and it gets a DIFFERENT
% EXECUTOR NAME rather than a widened `sprefa_extract` row: the contract's
% exact positional list is what pins which declarations the batching below is
% allowed to fold, and loosening it to "two or three columns" would silently
% admit a two-column declaration whose first column happened to be named repo.
%
% The clause order is the whole selection: a repo template ALSO ends in
% `{path}` when it ends in the file at all, and would otherwise be claimed by
% the unscoped row and then thrown out by its contract (measured:
% `host_executor_mismatch`, which is what a cold author saw before this row
% existed).
%
% CONTAINS, not ends-with, and the difference is a real declaration: the crawl
% bench's extraction host writes
% `"$DL_EXTRACT_BIN" ... {repo}/{path} >/dev/null && printf '%s\n' '{path}'`
% so that it answers ONE row per file rather than the extractor's whole JSONL.
% That is still the extractor run against `{repo}/{path}`; requiring the file
% to be the last thing on the line would have refused it for punctuation.
%
% THE ALTERNATIVE, priced and not taken: let the repo-scoped declaration fall
% to the generic `shell` executor. That needs no row at all -- writing the
% template as `'{repo}/{path}'` already fails the suffix test -- and it costs
% the applicative fold, so N named projections over one file become N
% subprocesses instead of one. It also makes a QUOTING CHARACTER decide which
% executor runs, which is the kind of silence this repo files as a defect.
host_execution(_, Template, sprefa_extract_repo) :-
    string(Template),
    sub_string(Template, 0, _, _, "\"$DL_EXTRACT_BIN\" "),
    sub_string(Template, _, 13, 0, "{repo}/{path}"),
    !.
host_execution(Name, Template, sprefa_extract) :-
    ( Name == extract
    ; string(Template),
      sub_string(Template, 0, _, _, "\"$DL_EXTRACT_BIN\" "),
      sub_string(Template, _, 6, 0, "{path}")
    ),
    !.
host_execution(_, _, shell).

host_executor_contract(sprefa_extract,
                       [col(path, text), col(digest, text)]).
host_executor_contract(sprefa_extract_repo,
                       [col(repo, text), col(path, text), col(digest, text)]).
host_executor_contract(shell, _).

% Ordinary `sh` inputs can serve two existing internal host roles. Identity
% inputs participate in both demand identity and witness digests and return on
% response rows. Freshness inputs retain the former salt behavior: they extend
% only the witness digest and stay on demand rows. The surface has one input
% list; these exact, positional contracts are compiler metadata.
host_input_contract(extract,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
% The repo-scoped twin. `repo` is identity for the same reason `path` is: it is
% part of what the answer is about and it returns on the response row. `digest`
% stays freshness, so the same content under two repositories is still two
% witnesses (the repo column is in the identity digest) while an unchanged file
% re-asked is still a cache hit.
host_input_contract(repo_extract,
                    [col(repo, text), col(path, text), col(digest, text)],
                    [identity, identity, freshness]).
% The org fan-out source (ruling org_fanout = repos_host_on_clock). `bucket` is
% the interval bind's column and it is FRESHNESS: it must not return on the
% response row (the answer is a repository name, not a name-and-a-clock-tick)
% and the template must not have to mention it. What it does is extend the
% witness, so each tick of the cadence re-asks and an unchanged org answer is
% absorbed as zero delta.
host_input_contract(repos,
                    [col(org, text), col(bucket, int)],
                    [identity, freshness]).
host_input_contract(gh_repos,
                    [col(org, text), col(bucket, int)],
                    [identity, freshness]).
host_input_contract(call_node,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(call_ref,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(df_node_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(df_edge_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(df_param_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(df_arg_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(call_node_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(type_node_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(sig_at,
                    [col(path, text), col(digest, text)],
                    [identity, freshness]).
host_input_contract(answer,
                    [col(name, text), col(bucket, int)],
                    [identity, freshness]).
host_input_contract(fetch,
                    [col(ep, text), col(prev, text), col(bucket, int)],
                    [identity, identity, freshness]).

% ═══ ghcacher hosts ═════════════════════════════════════════════════════════

% @comment-ok: user banner verbatim, ruling design_debt_mode = utility_over_pedantry.
% FUCKING WARNING: AI IS DUMB AND DECIDED THIS BECAUSE USER IS WANTING SOMETHING USEFUL AND IS DONE BEING PEDANTIC, DONT CAUSE CONTRADICTIONS WHEN POSSIBLE BUT FUCK IT
% Invocations stay serial (concatMap, concurrency 1), so neither clone row
% carries a concurrency column and no knob is registered for one.

% A second NAME for the mirror, never a mode column (repo_column_spelling).
% `want_sha` is FRESHNESS: an unmoved branch re-asks on the same witness.
host_input_contract(repo_checkout,
                    [col(repo_slug, text), col(dest_root, text),
                     col(want_sha, text)],
                    [identity, identity, freshness]).
host_input_contract(repo_mirror_pr_heads,
                    [col(repo_slug, text), col(dest_root, text),
                     col(want_sha, text)],
                    [identity, identity, freshness]).

% The config feeder pair. `bucket` is FRESHNESS on both: the answer is a file's
% existence and a file's contents, never a clock tick, so no template says it.
host_input_contract(path_exists,
                    [col(config_path, text), col(bucket, int)],
                    [identity, freshness]).
host_input_contract(read_org_config,
                    [col(config_path, text), col(bucket, int)],
                    [identity, freshness]).

% @comment-ok: user banner verbatim, ruling design_debt_mode = utility_over_pedantry.
% FUCKING WARNING: AI IS DUMB AND DECIDED THIS BECAUSE USER IS WANTING SOMETHING USEFUL AND IS DONE BEING PEDANTIC, DONT CAUSE CONTRADICTIONS WHEN POSSIBLE BUT FUCK IT
% Rate back-off is relational (an over_budget rel anti-joined into due), so
% there is no sleep, retry or back-off column here. Change detection is this one
% conditional call against the org events endpoint, so no second host is
% registered for the private-org fallback.

% One NAME per endpoint family. `prev_etag` is IDENTITY as `fetch`'s `prev` is,
% so it returns on the response row; a new bucket alone re-fires the poll.
host_input_contract(gh_rest_cond,
                    [col(endpoint_path, text), col(prev_etag, text),
                     col(bucket, int)],
                    [identity, identity, freshness]).

% @comment-ok: user banner verbatim, ruling design_debt_mode = utility_over_pedantry.
% FUCKING WARNING: AI IS DUMB AND DECIDED THIS BECAUSE USER IS WANTING SOMETHING USEFUL AND IS DONE BEING PEDANTIC, DONT CAUSE CONTRADICTIONS WHEN POSSIBLE BUT FUCK IT
% Serial again, one subprocess per batch: no concurrency column here either.

% The aliased batch. `slug_list` is IDENTITY: the set of repositories is what
% the answer is about, so it returns on the response row alongside `batch_key`.
host_input_contract(gh_pr_batch,
                    [col(batch_key, text), col(slug_list, text),
                     col(bucket, int)],
                    [identity, identity, freshness]).

% @comment-ok: user directive 2026-08-04 (env/pwd/git-root/config in dl6)
% ═══ env / filesystem / config hosts ════════════════════════════════════════
%
% Four ordinary sh hosts: the default `shell` executor runs them, the BODY
% lives at the call site, and these rows fix the input-role contracts only.
%
% env_var: name is identity and returns; bucket is freshness (re-asks).
host_input_contract(env_var,
                    [col(name, text), col(bucket, int)],
                    [identity, freshness]).
% pwd: no identity input, bucket alone is the freshness witness.
host_input_contract(pwd,
                    [col(bucket, int)],
                    [freshness]).
% git_toplevel: dir is identity and returns; not-a-repo answers zero rows.
host_input_contract(git_toplevel,
                    [col(dir, text), col(bucket, int)],
                    [identity, freshness]).
% toml_json: config_path is identity and returns; the doc is one json value.
host_input_contract(toml_json,
                    [col(config_path, text), col(bucket, int)],
                    [identity, freshness]).

host_input_roles(Name, Inputs, Roles) :-
    ( host_input_contract(Name, Inputs, ContractRoles)
    -> Roles = ContractRoles
    ; identity_roles(Inputs, Roles)
    ).

identity_roles([], []).
identity_roles([_ | Inputs], [identity | Roles]) :-
    identity_roles(Inputs, Roles).

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
% A body word that takes the shape of an ORDINARY relation atom and carries no
% wrapper syntax of its own: the parser reads it as the plain atom it looks
% like (no keyword_call clause, because wrapper_lower_role/3 fails on this
% shape) and the walk still reaches it, so a `reserved` row on this role
% refuses through 0_program_check.pl instead of becoming a silent empty EDB.
% `scan` is the first and the reason the role word exists.
body_lower_role(goal(_)).

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
            'validate a program through the text door; no server boots. Exit 0 clean, 2 named-unsupported construct findings, 1 broken (parse/compile error).').
cli_command(load,  '<file.dl6> [--port <port>]',
            'POST a compiled program to an already-running bop serve; exit 1 if nothing is listening.').
cli_command(q,     '<rel> [--port <port>] [--json]',
            'read one rel''s current rows from a running bop serve.').
cli_command(stats, '[--port <port>]',
            'read process and SQLite storage statistics from a running bop serve.').
cli_command(ticks, '[--port <port>]',
            'stream served tick events from a running bop serve until interrupted.').

% http_route(Method, Path, Summary). These are the server's public HTTP
% inventory. The TS route handlers remain explicit; generated metadata feeds
% CLI clients and detects inventory drift.
http_route('POST', '/program', 'compile and load a DL6 program.').
http_route('POST', '/edb/events', 'submit signed EDB events, one tick per call.').
http_route('GET',  '/idb/:rel', 'read one relation snapshot.').
http_route('GET',  '/ticks', 'stream tick events as SSE.').
http_route('GET',  '/stats', 'read process memory and SQLite storage statistics.').

% ═══ DL_PERF_LOG wire schema ════════════════════════════════════════════════
%
% trace_event(Name, Fields), Fields in WIRE ORDER
% field(Key, Type, Stability)
%
% One JSONL line per tick, under the DL_PERF_LOG flag every language in this
% repo already answers to (v6/prolog/6_profile.pl writes compile phases to it;
% v6/tsv2/serve/0_trace.ts writes ticks to it). This table is what makes a
% SECOND emitter reproducible: it fixes the key spelling and the key order so a
% rust runtime's `tracing` layer and this one's pino destination write the same
% bytes for the same program and schedule.
%
% Stability says whether a field belongs to that byte comparison:
%   stable  every emitter must reproduce it exactly
%   timing  a clock, stripped before the cross-target diff
%   host    target-specific text (an error rendering), stripped with it
%
% Key spelling is lower snake case and every elapsed value ends _ms, which is
% the convention 6_profile.pl:7-9 states and the TS side had drifted from
% (`ms`, `witnessDigest`). WIRE keys only: TS locals stay camelCase.
% `actor`/`seam` sit on the LINE and on no nested record; the tick channel
% event is the tick_line prefix before `actor` (traceSchema.test.ts slices it).
trace_event(tick_line,
            [ field(tick,       int,           stable),
              field(rels,       int,           stable),
              field(rows,       int,           stable),
              field(statements, int,           stable),
              field(wall_ms,    real,          timing),
              field(actor,      text,          stable),
              field(seam,       text,          stable),
              field(rules,      list(rule),    stable),
              field(effects,    list(effect),  stable),
              field(binds,      list(bind),    stable),
              field(watches,    list(watch),   stable) ]).
trace_event(rule,
            [ field(rule,    text, stable),
              field(rows,    int,  stable),
              field(wall_ms, real, timing) ]).
trace_event(effect,
            [ field(host,           text, stable),
              field(witness_digest, text, stable),
              field(outcome,        text, stable),
              field(rows,           int,  stable),
              field(wall_ms,        real, timing),
              field(error,          text, host) ]).
trace_event(bind,
            [ field(rel,    text, stable),
              field(period, int,  stable),
              field(bucket, int,  stable) ]).
trace_event(watch,
            [ field(rel,     text, stable),
              field(glob,    text, stable),
              field(added,   int,  stable),
              field(removed, int,  stable) ]).
