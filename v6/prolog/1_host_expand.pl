% Selected host, bind, probe, and query term forms.
%
% Phase 1 turns a probe into ordinary relations:
%
%   __host_demand_Name(Identity, Witness, Inputs..., Salts...)
%   __host_response_Name(Witness, Ordinal, Inputs..., Outputs...)
%
% The demand relation is derived by a level rule. The response relation is
% EDB, keyed by (Witness, Ordinal), and receives answers through the fixture
% schedule or, when served, from the host runner.
%
% Host answers are keyed by (Witness, Ordinal), so multiple rows for one
% witness coexist and late answers replace rows positionally.
%
% A late answer with fewer rows leaves surplus ordinals; content-addressed
% witnesses prevent this under the declared schedule contract.

:- module(host_expand,
          [ prepare_program/5,
            compile_host_decl/2,
            compile_host_decl/3,
            host_plan_contract/2,
            compile_query/2,
            query_decl/3,
            compile_ts_query/2,
            body_goals/2,
            host_relation_refs/3,
            reserved_host_column/1
          ]).

:- use_module(library(lists)).
:- use_module(library(pairs)).
:- use_module('0_body_walk', [body_conjunction_goals/3]).
:- use_module('compile/registry',
              [ arrival_roles/3,
                host_input_roles/3
              ]).
:- use_module('next/0_parse/0_cst_query', [ serialize_ts_query/2 ]).
:- use_module('0_dot_expand', [resolve_relation_paths/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

prepare_program(Input, prog(Decls, Rules), HostPlans, [], QueryPlans) :-
    program_parts(Input, RawDecls, RawRules, Queries),
    resolve_relation_paths(RawDecls, Queries, ResolvedQueries),
    maplist(normalize_rule, RawRules, NormalizedRules),
    findall(HostPlan,
            ( member(Decl, RawDecls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, RawDecls, HostPlan)
            ),
            HostPlans),
    no_duplicate_host_names(HostPlans),
    no_host_rel_name_collisions(HostPlans, RawDecls),
    no_host_rule_heads(HostPlans, NormalizedRules),
    maplist(compile_query, ResolvedQueries, QueryPlans),
    expand_probe_rules(NormalizedRules, HostPlans, RawDecls,
                       ExpandedRules, GeneratedDecls),
    unprobed_host_decls(HostPlans, GeneratedDecls, UnprobedDecls),
    append([RawDecls, ResolvedQueries, GeneratedDecls, UnprobedDecls],
           Decls0),
    dedupe_terms(Decls0, Decls),
    append(ExpandedRules, [], Rules).

program_parts(prog(Decls, Rules), Decls, Rules, []).
program_parts(program(Decls, Rules, Queries), Decls, Rules, Queries).

% A HOST NAME IS A KEY, so it has to be unique, and until 2026-07-31 nothing
% said so.
%
% FAIL-FIRST RECEIPT (ARCH row host_arity_overload_miscompile, measured by the
% files lane before this predicate existed). Two declarations, one name:
%
%   sh look(path: text)             -> (line: text) = `head -1 {path}`.
%   sh look(repo: text, path: text) -> (line: text) = `head -1 {repo}/{path}`.
%
% compiled CLEAN, exit 0, on BOTH doors. host_relation_refs/3 derives the two
% generated relation names from the NAME alone, so both plans point at
% __host_demand_look and __host_response_look; the second declaration's `repo`
% column has nowhere to live, and which arity a probe resolves against comes
% down to which plan the lookup reaches first. Nothing in the emitted module
% reads as wrong -- there is simply one relation where the author wrote two.
%
% The overload that a cold author reaches for is what the repo-scoped pair in
% fixtures/files-hosts.dl6 spells instead, and this unsupported construct is the other half of
% ruling repo_column_spelling = distinct_name_hosts: the ruling says the repo
% case gets its own name, and this says the language will not let it not.
% A DECLARED HOST DECLARES ITS RELATIONS, probe or no probe.
%
% `generated_host_decls/7` is reached from `expand_probe_rules/5`, so until
% 2026-07-31 a host that no rule probed produced a host PLAN naming
% __host_demand_<name> and __host_response_<name> and no DECLARATION of either.
%
% FAIL-FIRST RECEIPT, found by the files/repos lane writing exactly the shape
% the org_fanout ruling asks for -- a gh-shaped `repos` template declared
% alongside the local one, written but not yet consumed:
%
%   POST /program -> 200 {"loaded":true, ..., "hosts":[...,"gh_repos",...]}
%   Error: unknown rel '__host_demand_gh_repos' in program '44a6494405...'
%     at serve/3_engine.ts:143
%
% and the served process DIED. The load answered success, the boot demand scan
% then asked the engine for a relation the compiler never declared, and the
% error tore the whole subscription down -- a 200 followed by a dead server,
% which is the self-diagnosis law's exact complaint.
%
% The fix is the reading that matches `rel`: an undemanded declaration is an
% EMPTY relation, not an absent one (ruling edb_definition says the same thing
% about an unheaded relation atom). So a host with no probe gets its demand and
% response relations at their base arity -- no salts, because a salt only exists
% at a probe -- and they simply stay empty. That is what lets a program stage a
% declaration it does not use yet, which is what the gh variant of `repos` is.
%
% The arity guard matters and is why this is not "always emit base decls":
% salts widen the demand relation, so emitting a base-arity twin beside a
% probed host would declare a SECOND relation under the same name at a
% different arity. Hosts already declared by a probe are skipped.
unprobed_host_decls(HostPlans, GeneratedDecls, UnprobedDecls) :-
    findall(Decls,
            ( member(host_plan(_, Inputs, Outputs, _,
                               demand_ref(DemandName), response_ref(ResponseName), _),
                     HostPlans),
              \+ already_declared(DemandName, GeneratedDecls),
              generated_host_decls(DemandName, ResponseName, Inputs, Outputs,
                                   [], [], Decls)
            ),
            DeclLists),
    append(DeclLists, UnprobedDecls).

already_declared(DemandName, Decls) :-
    member(col_type(DemandName/_, _, _), Decls),
    !.

no_duplicate_host_names(HostPlans) :-
    findall(Name, member(host_plan(Name, _, _, _, _, _, _), HostPlans), Names),
    ( duplicate(Names, Name)
    -> throw(duplicate_host_decl(Name))
    ; true
    ).

% One name is one declaration: a same-named ordinary rel used to be created,
% writable, and never read, because the body atom rewrote to probe/4.
no_host_rel_name_collisions(HostPlans, RawDecls) :-
    ( member(host_plan(Name, _, _, _, _, _, _), HostPlans),
      member(col_type(Name/_, _, _), RawDecls)
    -> throw(host_and_rel_share_name(Name))
    ; true
    ).

% An arrival rel with a rule head is two sources writing one rel; today the
% arrow would be silently ignored (compile.pl subtracts DerivedRefs), so stop.
no_host_rule_heads(HostPlans, Rules) :-
    ( member(host_plan(Name, _, _, _, _, _, _), HostPlans),
      member(Rule, Rules),
      rule_head_name(Rule, Name)
    -> throw(host_and_rule_head(Name))
    ; true
    ).

rule_head_name((Head <- _), Name) :- functor(Head, Name, _).
rule_head_name((Head <+ _), Name) :- functor(Head, Name, _).

normalize_rule(Raw, Rule) :-
    normalize_rule_shape(Raw, Shaped),
    compile_value_terms(Shaped, Rule).

normalize_rule_shape(fact(Head), (Head <- true)) :- !.
normalize_rule_shape(rule(Head, Items), (Head <- Body)) :- !,
    body_from_list(Items, Body).
normalize_rule_shape(match(Source, Arms), match(Source, Arms)) :- !.
normalize_rule_shape((Head <- Body), (Head <- Body)).
normalize_rule_shape((Head <+ Body), (Head <+ Body)).

compile_value_terms(Term, Term) :-
    var(Term),
    !.
compile_value_terms(Term, Text) :-
    nonvar(Term),
    ( Term = cst(_, _, _, _)
    ; Term = cst(_, _, _, _, _)
    ),
    !,
    Text = Term.
compile_value_terms(Term, Text) :-
    nonvar(Term),
    Term = ts_query(_),
    !,
    compile_ts_query(Term, Text).
compile_value_terms(Term, Term) :-
    atomic(Term),
    !.
compile_value_terms(Term, Compiled) :-
    Term =.. [Name | Args],
    maplist(compile_value_terms, Args, CompiledArgs),
    Compiled =.. [Name | CompiledArgs].

body_from_list([], true).
body_from_list([Only], Only) :- !.
body_from_list([First | Rest], (First, Body)) :-
    body_from_list(Rest, Body).

compile_host_decl(Decl, HostPlan) :-
    compile_host_decl(Decl, [], HostPlan).

% Context is the raw decl list: an arrival_identity/2 riding beside the
% sh_decl (the surface key(P..)) wins over the registry's contract rows.
compile_host_decl(
    sh_decl(Name, Inputs, Outputs, template(Template)),
    Context,
    host_plan(Name, Inputs, Outputs, template(Template),
              demand_ref(DemandRef), response_ref(ResponseRef),
              input_roles(Roles))) :-
    atom(Name),
    string(Template),
    validate_columns(Inputs, input),
    validate_columns(Outputs, output),
    column_names(Inputs, InputNames),
    column_names(Outputs, OutputNames),
    disjoint_columns(InputNames, OutputNames),
    no_reserved_columns(Name, input, InputNames),
    no_reserved_columns(Name, output, OutputNames),
    ( memberchk(arrival_identity(Name, Positions), Context)
    -> arrival_roles(Inputs, Positions, Roles)
    ;  host_input_roles(Name, Inputs, Roles)
    ),
    validate_template(Template, InputNames, OutputNames, Roles),
    host_relation_refs(Name, DemandRef, ResponseRef),
    !.
compile_host_decl(Decl, _, _) :-
    throw(refused_host_decl(Decl)).

% The legacy host_plan/7 term remains the shared lowering shape used by the
% oracle and the two emitters.  Its typed boundary companion is derived here,
% at the same compiler seam, so request and response layouts do not get
% reconstructed from emitted JSON.  `ref` names the generated catalog port
% relation and its arity; fields retain the authored order and declared type.
host_plan_contract(
    host_plan(Name, Inputs, Outputs, _Template,
              demand_ref(DemandRef), response_ref(ResponseRef), _Roles),
    host_contract(
        type_descriptor(DemandRef/RequestArity, RequestFields),
        type_descriptor(ResponseRef/ResponseArity, ResponseFields))) :-
    length(Inputs, RequestArity),
    length(Outputs, ResponseArity),
    maplist(host_field, Inputs, RequestFields),
    maplist(host_field, Outputs, ResponseFields),
    atom(Name).

host_field(col(Name, Type), field(Name, Type)).

validate_columns(Columns, Role) :-
    is_list(Columns),
    column_names(Columns, Names),
    ( duplicate(Names, Name)
    -> throw(column_mismatch(Role, duplicate(Name)))
    % STRUCT-AS-ROWS: a host column type may name a DECLARED struct type as
    % well as a primitive. Whether the name resolves is not this predicate's
    % question -- 0_program_check.pl:column_type_unknown answers it for every
    % col_type/3 in the program, host-produced entries included -- so the test
    % here is only that a type was spelled at all.
    ; forall(member(col(Name, Type), Columns), ( atom(Name), atom(Type) ))
    ).

column_names(Columns, Names) :-
    maplist(column_name, Columns, Names).

column_name(col(Name, _), Name).

duplicate(Names, Name) :-
    select(Name, Names, Rest),
    memberchk(Name, Rest),
    !.

disjoint_columns(Inputs, Outputs) :-
    ( member(Name, Inputs), memberchk(Name, Outputs)
    -> throw(column_mismatch(input_output_overlap(Name)))
    ; true
    ).

% A host column that collides with one the RUNTIME puts on the same relation.
% The sibling of disjoint_columns/2 above: same kind of collision, except the
% other party is not another column the author wrote, it is this file's own
% generated_host_decls/7.
%
% FAIL-FIRST RECEIPT. `sh look(path: text) -> (ordinal: int)` compiled clean
% and produced a response relation of ARITY 4 carrying THREE column names:
%
%   col __host_response_look/4 . witness_digest : text
%   col __host_response_look/4 . ordinal        : int
%   col __host_response_look/4 . path           : text
%                                                   <- the author's `ordinal`
%
% against the control, which carries four:
%
%   col __host_response_plain/4 . witness_digest, ordinal, path, line
%
% The author's column did not clash and lose, it VANISHED: two identical
% col_type/3 terms were folded by dedupe_terms/2 while the arity kept the
% slot, so one position of the relation had no declaration at all.
%
% Downstream the same name decides where a value goes. serve/1_hosts.ts
% project() fills each response column BY LITERAL NAME, and the runtime names
% are tested FIRST:
%
%   if (column === "witness_digest") return witnessDigest;
%   if (column === "ordinal") return ordinal;
%   const input = demand.inputs.get(column); ...
%
% so a host output called `ordinal` can never receive its own value -- it gets
% the answer's row index -- and an input called `witness_digest` gets the
% digest rather than what the rule bound. No error, no trace, a program that
% derives the wrong thing or nothing.
%
% The three names are stated here rather than derived because they are stated
% in two other places already (generated_host_decls/7 below builds the
% columns, project() above fills them) and a fourth restatement that could
% silently disagree is exactly what this unsupported construct exists to prevent; the
% host_reserved_column_inventory unit pins this list against the columns
% generated_host_decls/7 actually emits.
%
% SCOPE, checked rather than assumed: the two lists here cover every column
% that reaches either generated relation. A probe SALT also lands on the
% demand relation, and it needs no separate check because a salt is not a new
% name -- salts_match_columns/2 requires each salt to name a declared INPUT
% column (the freshness-role partition of this same list), so a salt spelling
% a reserved name is refused here, at the declaration, before any probe
% mentions it.
reserved_host_column(witness_digest).
reserved_host_column(ordinal).
reserved_host_column(identity_digest).

no_reserved_columns(Host, Role, Names) :-
    (   member(Name, Names), reserved_host_column(Name)
    ->  throw(host_column_shadows_runtime(Host, Role, Name))
    ;   true
    ).

% An arrival rel carries no command line; only a term-door template is checked.
validate_template("", _, _, _) :- !.
validate_template(Template, Inputs, Outputs, Roles) :-
    role_names(identity, Inputs, Roles, IdentityInputs),
    ( member(Name, IdentityInputs), \+ template_mentions(Template, Name)
    -> throw(template_mismatch(unreferenced_input(Name)))
    ; member(Name, Outputs), template_mentions(Template, Name)
    -> throw(template_mismatch(output_used_as_input(Name)))
    ; brace_template_reference(Template, Name), \+ memberchk(Name, Inputs)
    -> throw(template_mismatch(unknown_column(Name)))
    ; true
    ).

role_names(_, [], [], []).
role_names(Role, [Name | Inputs], [Here | Roles], Names) :-
    ( Here == Role
    -> Names = [Name | Rest]
    ; Rest = Names
    ),
    role_names(Role, Inputs, Roles, Rest).

template_mentions(Template, Name) :-
    template_reference(Template, Name),
    !.

template_reference(Template, Name) :-
    string_codes(Template, Codes),
    ( brace_reference(Codes, Name)
    ; dollar_reference(Codes, Name)
    ).

brace_template_reference(Template, Name) :-
    string_codes(Template, Codes),
    brace_reference(Codes, Name).

brace_reference(Codes, Name) :-
    append(_, [0'{ | Tail], Codes),
    append(NameCodes, [0'} | _], Tail),
    NameCodes = [First | _],
    identifier_start_code(First),
    maplist(identifier_code, NameCodes),
    atom_codes(Name, NameCodes).

dollar_reference(Codes, Name) :-
    append(_, [0'$ , First | Tail], Codes),
    identifier_start_code(First),
    take_identifier_codes(Tail, Rest, _),
    atom_codes(Name, [First | Rest]).

identifier_start_code(Code) :-
    code_type(Code, alpha)
    ; Code =:= 0'_.

identifier_code(Code) :-
    code_type(Code, alnum)
    ; Code =:= 0'_.

take_identifier_codes([Code | Rest], [Code | More], Tail) :-
    identifier_code(Code),
    !,
    take_identifier_codes(Rest, More, Tail).
take_identifier_codes(Tail, [], Tail).

host_relation_refs(Name, DemandRef, ResponseRef) :-
    atom_concat('__host_demand_', Name, DemandName),
    atom_concat('__host_response_', Name, ResponseName),
    DemandRef = DemandName,
    ResponseRef = ResponseName.

% The ONE reader of both `?` decl shapes; order rides the final cursor alone,
% so a query plan is the same term with or without a tail.
query_decl(query(Atom), Atom, []).
query_decl(query(Atom, order(OrderCols)), Atom, OrderCols).

compile_query(Query,
              query_plan(Name/Arity, columns(Args), snapshot(current))) :-
    query_decl(Query, Atom, _),
    compound(Atom),
    Atom =.. [Name | Args],
    atom(Name),
    length(Args, Arity),
    !.
compile_query(Query, _) :-
    throw(query_mismatch(Query)).

compile_ts_query(ts_query(Patterns), Text) :-
    serialize_ts_query(ts_query(Patterns), Text).
ts_escaped_codes([Code | Rest]) --> [Code], ts_escaped_codes(Rest).

expand_probe_rules([], _, _, [], []).
expand_probe_rules([Rule | Rest], HostPlans, RawDecls,
                   Expanded, GeneratedDecls) :-
    expand_probe_rule(Rule, HostPlans, RawDecls,
                      RuleExpanded, RuleDecls),
    expand_probe_rules(Rest, HostPlans, RawDecls,
                       RestExpanded, RestDecls),
    append(RuleExpanded, RestExpanded, Expanded),
    append(RuleDecls, RestDecls, GeneratedDecls).

expand_probe_rule((Head <- Body), HostPlans, RawDecls,
                  [DemandRule, (Head <- JoinedBody)], Decls) :-
    body_goals(Body, Goals),
    append(BeforeProbe, [Probe | AfterProbe], Goals),
    Probe = probe(_, _, _, _),
    !,
    append(BeforeProbe, AfterProbe, RemainingGoals),
    ( member(OtherProbe, RemainingGoals), OtherProbe = probe(_, _, _, _)
    -> throw(probe_mismatch(multiple_probes(Body)))
    ; true
    ),
    expand_probe(Probe, HostPlans, RawDecls,
                 DemandAtom, WitnessBind, ResponseAtom, Decls),
    body_from_list(BeforeProbe, DemandBody),
    append([BeforeProbe, [WitnessBind, ResponseAtom], AfterProbe], JoinedGoals),
    body_from_list(JoinedGoals, JoinedBody),
    DemandRule = (DemandAtom <- DemandBody).
expand_probe_rule(Rule, _, _, [Rule], []).

% The plainest walk in the tree: flatten the conjunction spine and leave every
% wrapper whole (rank R1 of plans/2026-07-29-prolog-org-review.md). Probe
% selection above depends on wrappers staying intact, so nothing is spliced and
% not/1 is not descended.
body_goals(Body, Goals) :-
    body_conjunction_goals(Body,
                           walk_policy(descend_not(false),
                                       splice_bare(false)),
                           Goals).

expand_probe(Probe, HostPlans, RawDecls,
             DemandAtom, WitnessBind, ResponseAtom, Decls) :-
    Probe = probe(Name, InputValues, OutputValues, Salts),
    ( member(HostPlan, HostPlans),
      HostPlan = host_plan(Name, SurfaceInputs, Outputs, _,
                           demand_ref(DemandName),
                           response_ref(ResponseName),
                           input_roles(Roles)),
      partition_host_columns(SurfaceInputs, Roles, Inputs, FreshnessInputs)
    -> true
    ; throw(probe_mismatch(Probe))
    ),
    ( same_length(Inputs, InputValues),
      same_length(Outputs, OutputValues),
      salts_match_columns(FreshnessInputs, Salts)
    -> true
    ; throw(probe_mismatch(Probe))
    ),
    digest_expr(identity, Name, Inputs, InputValues, [], Identity),
    digest_expr(witness, Name, Inputs, InputValues, Salts, Witness),
    salt_values(Salts, SaltValues),
    append([Identity, Witness | InputValues], SaltValues, DemandArgs),
    DemandAtom =.. [DemandName | DemandArgs],
    WitnessBind = (WitnessValue := Witness),
    % _Ordinal is a FRESH variable in every probe expansion: rules never write
    % it and never read it, they only refuse to collapse on it. See
    % generated_host_decls/7 below for why the column exists at all.
    append([WitnessValue, _Ordinal | InputValues], OutputValues, ResponseArgs),
    ResponseAtom =.. [ResponseName | ResponseArgs],
    generated_host_decls(DemandName, ResponseName, Inputs, Outputs,
                         Salts, RawDecls, Decls).

digest_expr(Role, Name, Inputs, InputValues, Salts, concat(Parts)) :-
    format(string(Prefix), "~w|~w", [Role, Name]),
    input_digest_parts(Inputs, InputValues, InputParts),
    salt_digest_parts(Salts, SaltParts),
    append([[Prefix], InputParts, SaltParts], Parts).

input_digest_parts([], [], []).
input_digest_parts([col(Name, Type) | Cols], [Value | Values],
                   ["|", Name, ":", Type, "=", Value | Parts]) :-
    input_digest_parts(Cols, Values, Parts).

partition_host_columns([], [], [], []).
partition_host_columns([Column | Columns], [Role | Roles],
                       Identity, Freshness) :-
    ( Role == identity
    -> Identity = [Column | IdentityRest],
       Freshness = FreshnessRest
    ; Role == freshness
    -> Freshness = [Column | FreshnessRest],
       Identity = IdentityRest
    ),
    partition_host_columns(Columns, Roles, IdentityRest, FreshnessRest).

salts_match_columns(Columns, Salts) :-
    same_length(Columns, Salts),
    maplist(salt_matches_column, Columns, Salts),
    valid_salts(Salts).

salt_matches_column(col(Name, _), salt(Name, _)).

salt_digest_parts([], []).
salt_digest_parts([salt(Name, Value) | Rest],
                  ["|", Name, "=", Value | Parts]) :-
    salt_digest_parts(Rest, Parts).

valid_salts(Salts) :-
    is_list(Salts),
    forall(member(salt(Name, _), Salts), atom(Name)),
    findall(Name, member(salt(Name, _), Salts), Names),
    \+ duplicate(Names, _).

salt_values([], []).
salt_values([salt(_, Value) | Rest], [Value | Values]) :-
    salt_values(Rest, Values).

generated_host_decls(DemandName, ResponseName, Inputs, Outputs,
                     Salts, RawDecls, Decls) :-
    length(Inputs, InputCount),
    length(Salts, SaltCount),
    DemandArity is 2 + InputCount + SaltCount,
    length(Outputs, OutputCount),
    ResponseArity is 2 + InputCount + OutputCount,
    DemandRef = DemandName/DemandArity,
    ResponseRef = ResponseName/ResponseArity,
    column_type_decls(DemandRef,
                      [col(identity_digest, text), col(witness_digest, text) | Inputs],
                      DemandBaseDecls),
    salt_column_decls(DemandRef, Salts, RawDecls, SaltDecls),
    column_type_decls(ResponseRef,
                      [col(witness_digest, text), col(ordinal, int) | Inputs],
                      ResponseInputDecls),
    column_type_decls(ResponseRef, Outputs, ResponseOutputDecls),
    append([[keyed(ResponseRef, [1, 2])],
            DemandBaseDecls, SaltDecls,
            ResponseInputDecls, ResponseOutputDecls],
           Decls).

column_type_decls(_, [], []).
column_type_decls(Ref, [col(Name, Type) | Rest],
                  [col_type(Ref, Name, Type) | Decls]) :-
    column_type_decls(Ref, Rest, Decls).

salt_column_decls(_, [], _, []).
salt_column_decls(Ref, [salt(Name, _) | Rest], RawDecls,
                  [col_type(Ref, Name, Type) | Decls]) :-
    ( declared_column_type(RawDecls, Name, FoundType)
    -> Type = FoundType
    ; Type = text
    ),
    salt_column_decls(Ref, Rest, RawDecls, Decls).

declared_column_type(Decls, Name, Type) :-
    member(col_type(_, Name, Type), Decls),
    !.
declared_column_type(Decls, Name, Type) :-
    member(sh_decl(_, Inputs, Outputs, _), Decls),
    ( memberchk(col(Name, Type), Inputs)
    ; memberchk(col(Name, Type), Outputs)
    ),
    !.

% First occurrence wins and the surviving order is the input order. The scan
% form below is quadratic in the declaration count and measured 15.1 ms on
% pokeapi's 1246 declarations; sorting on a positional key measures 0.29 ms and
% answers the same list whenever the terms are ground, where memberchk/2's
% unification and ==/2 agree. A non-ground term can unify with an earlier one
% AND bind it, so that case keeps the scan.
dedupe_terms(Terms, Deduped) :-
    (   ground(Terms)
    ->  ground_dedupe_terms(Terms, Deduped)
    ;   dedupe_terms(Terms, [], Deduped)
    ).

ground_dedupe_terms(Terms, Deduped) :-
    positional_terms(Terms, 0, Positioned),
    sort(1, @<, Positioned, Unique),
    sort(2, @<, Unique, InInputOrder),
    pairs_keys(InInputOrder, Deduped).

positional_terms([], _, []).
positional_terms([Term | Rest], Position, [Term-Position | Out]) :-
    Next is Position + 1,
    positional_terms(Rest, Next, Out).

dedupe_terms([], _, []).
dedupe_terms([Term | Rest], Seen, Deduped) :-
    ( memberchk(Term, Seen)
    -> dedupe_terms(Rest, Seen, Deduped)
    ; Deduped = [Term | More],
      dedupe_terms(Rest, [Term | Seen], More)
    ).
