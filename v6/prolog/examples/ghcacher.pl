% ghcacher.pl : examples/gh-cache.dl (141 lines, v5) reimagined in the
% kernel-4 surface. THIS FILE IS THE PROGRAM; the checks it runs are imported
% from ../src/ (kernel.pl, checks.pl, grader.pl), not restated.
%
% Run:  swipl -q -l v6/prolog/examples/ghcacher.pl -g go -g halt
%       swipl -q -l v6/prolog/examples/ghcacher.pl -g report -g halt
%
% What died from v5 and what killed it:
%   etag/etag_next twin + @next carry        -> the register's `pre`
%   resp + resp_latest + resp_current +      -> register arms; a register IS
%     max(bucket) latest-wins chain             latest-wins by construction
%   "304 keeps old etag" (prose warning)     -> the `unchanged` arm, checked
%   status==200 filters downstream           -> the envelope: only `fresh` has a body
%   change_log/change_log_next + self-union  -> persistent monotone rel (count-IVM)
%   @async/@next annotations, clock builtin  -> time is a FIELD (every_300.bucket)
% Kept from v5 (it was already right): content-addressed request identity,
% (ep, prev, bucket) -> one request id = node-hash sharing = the rate cap.

:- use_module('../src/kernel.pl').
:- use_module('../src/checks.pl').
:- use_module('../src/grader.pl').

:- op(1150, xfx, <-).
:- discontiguous rule/1.

% ═══ value types ════════════════════════════════════════════════════════════

enum(fetch_result,
  [ fresh(tag, body)          % 200 with a new representation
  , unchanged                 % 304: free, and must be handled
  , error(status)             % anything else; no body by construction
  ]).

% ═══ sources and externals (the only impure declarations) ═══════════════════

source(watch,      [ep]).
source(every_300,  [bucket]).        % the cadence clock: a rel with a field
source(subscriber, [client]).

external(fetch, [ep, prev], fetch_result).
external(sse,   [client, row]).

fact(watch("repos/cli/cli")).

% ═══ island 1: what to ask ══════════════════════════════════════════════════
% The bucket join is the rate cap: same (ep, prev, bucket) = same request id
% = at most one wire call per 300s per endpoint.

rule(poll(Ep, Prev, B) <- (watch(Ep), cache_tag(Ep, Prev), every_300(B))).

% ═══ the register: the entire v5 cache apparatus ════════════════════════════

register(cache(ep), over(fetch),
  [ arm(subscribe,               entry("", no_body))
  , arm(next(fresh(Tag, Body)),  pre(_), entry(Tag, body(Body)))
  , arm(next(unchanged),         pre(S), S)
  , arm(next(error(_)),          pre(S), S)
  ]).

rule(cache_tag(Ep, Tag)   <- cache(Ep, entry(Tag, _))).
rule(cache_body(Ep, Body) <- cache(Ep, entry(_, body(Body)))).

% ═══ island 2: normalize entities from the current body ═════════════════════

rule(stars(Ep, N)     <- (cache_body(Ep, Body), jsonp(Body, "stargazers_count", N))).
rule(full_name(Ep, V) <- (cache_body(Ep, Body), jsonp(Body, "full_name", V))).
rule(pull_request(Ep, Num, Title, State, Author) <-
       (cache_body(Ep, Body),
        json(Body, each_obj([ number-Num, title-Title, state-State,
                              user-obj([login-Author]) ])))).

% ═══ the change feed: append-only, monotone, no twin ════════════════════════

rule(change_log(Ep, "stars", N)     <- stars(Ep, N)).
rule(change_log(Ep, "full_name", V) <- full_name(Ep, V)).

% ═══ boundary out: SSE tails the deltas ═════════════════════════════════════

rule(sse(Client, row(Ep, Kind, Val)) <-
       (subscriber(Client), delta(change_log(Ep, Kind, Val)))).

% ═══ checks (imported logic, program-local wiring) ══════════════════════════

check(envelope_total, ( register(_, over(Ext), Arms),
                        external(Ext, _, EnumName), enum(EnumName, Ctors),
                        covers_enum(Arms, Ctors),
                        has_subscribe_arm(Arms) )).
check(kernel_only,    ( surface_grounded )).
check(twins_dead,     ( findall(Head <- Body, rule(Head <- Body), Rules),
                        findall(Head, member(Head <- _, Rules), Heads),
                        no_twin_names(Heads),
                        no_self_union(Rules) )).

go :- run(check).

% ═══ archipelago report ═════════════════════════════════════════════════════

report :-
    aggregate_all(count, rule(_), Rules),
    aggregate_all(count, register(_, _, _), Registers),
    aggregate_all(count, external(_, _, _), TypedExt),
    aggregate_all(count, external(_, _), PlainExt),
    Externals is TypedExt + PlainExt,
    format("rules (sqlite islands, IVM-maintained): ~w~n", [Rules]),
    format("registers (temporal, delta-fed):        ~w~n", [Registers]),
    format("externals (host boundary):              ~w~n", [Externals]),
    forall(register(Name, over(Src), _),
           format("  register ~w folds external ~w~n", [Name, Src])),
    forall(( rule(Head <- Body), checks:body_member(delta(_), Body),
             functor(Head, HeadName, _) ),
           format("  boundary-out ~w is clocked on deltas~n", [HeadName])),
    format("v5: 12 rels (2 twins + 3-rel latest-wins chain) -> ~w rules + 1 register~n",
           [Rules]).
