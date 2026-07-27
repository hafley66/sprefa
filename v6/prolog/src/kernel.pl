% kernel.pl : the syntax kernel and sugar registry, as a module. The single
% source of truth for "what is core"; ARCH.pl and every example checker import
% it instead of restating it.

:- module(kernel,
          [ kernel/1, sugar/2, grounds/1, surface_form/2 ]).

:- use_module(library(lists)).
:- table grounds/1.

% ── the four primitives ─────────────────────────────────────────────────────

kernel(ground_terms).   % structs/tuples/terms, incl. terms-in-columns
kernel(rule).           % head <- body: join, selection, stratified negation
kernel(register).       % pre/carry: the single stateful primitive
kernel(external_rel).   % the effect boundary: host reads/writes rows

% ── sugar(Feature, DesugarsInto): every chain must ground out in kernel/1 ───

sugar(clock_on,        [ground_terms, rule]).          % tick FIELD + a join; no @
sugar(next_arrow,      [register]).
sugar(async_arrow,     [external_rel, clock_on]).
sugar(envelope_match,  [rule, ground_terms]).
sugar(match_over_time, [register, envelope_match]).
sugar(fold_reduce,     [match_over_time]).
sugar(delay,           [rule, clock_on]).
sugar(throttle,        [clock_on]).
sugar(absence,         [rule, clock_on]).
sugar(when_thin,       [clock_on, rule]).
sugar(merge_clocks,    [when_thin, rule]).
sugar(with_latest_from,[register, rule, clock_on]).
sugar(sample,          [register, clock_on]).
sugar(combine_latest,  [with_latest_from]).
sugar(debounce,        [delay, register]).
sugar(switch_map,      [clock_on, external_rel]).
sugar(demand_lazy,     [rule, clock_on]).
sugar(impure_loop,     [register, external_rel, rule, clock_on]).

grounds(Feature) :- kernel(Feature).
grounds(Feature) :- sugar(Feature, Parts), forall(member(P, Parts), grounds(P)).

% ── the surface declaration forms and what kernel parts they map onto ───────

surface_form(fact,     [ground_terms]).
surface_form(source,   [external_rel]).
surface_form(external, [external_rel, ground_terms]).
surface_form(enum,     [ground_terms]).
surface_form(rule,     [rule]).
surface_form(register, [register, ground_terms]).
