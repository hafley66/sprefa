% 3_derived.pl : case C6 evidence. What does the compiler actually HAVE for a
% derived relation at the moment a declaration-time splice would need its
% columns, and what does the column inference hand back once it runs?
%
% This module does not design around the gap. It reads the real
% compile/analyze.pl and the real expansion order and records the failure
% shape.
%
% Phase order in the shipped tree (both consumers call the expansion FIRST):
%   compile/compile.pl:92        expand_match_program(SugaredProg, Prog)
%   conformance/engine.pl:349    expand_match_program(SugaredProg, Prog)
%   0_match_expand.pl:20         expand_enum_program(...) inside it
% Column names for an undeclared ref come from analyze.pl:rel_columns/4,
% which needs Rules PLUS the surface variable Bindings the caller recovers
% with read_term(..., [variable_names(Bindings)]). Neither is in scope where
% the expansion runs, and Bindings does not exist at all for a program that
% arrived as a term rather than as text.

:- module(derived_source,
          [ inferred_columns_for_derived_rel/2,
            inferred_columns_for_anonymous_rel/2,
            enum_generated_columns/2
          ]).

:- use_module(library(lists)).
:- use_module('../../compile/analyze', [rel_columns/4]).
:- use_module('../../0_enum_expand', [expand_enum_program/2]).
:- use_module('0_spread', [declared_columns/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% What inference hands you for a DERIVED rel, given the surface text (the
% only place the names live).
inferred_columns_for_derived_rel(Ref, Columns) :-
    read_rules("[(pair(Id, Tag) <- source(Id, Tag))]", Rules, Bindings),
    rel_columns(Rules, Bindings, Ref, Columns).

% And for a derived rel whose positions are never plain named variables.
inferred_columns_for_anonymous_rel(Ref, Columns) :-
    read_rules("[(pair(_, 'lit') <- source(_, _))]", Rules, Bindings),
    rel_columns(Rules, Bindings, Ref, Columns).

read_rules(Text, Rules, Bindings) :-
    read_term_from_atom(Text, Rules, [variable_names(Bindings)]).

% Contrast: enum expansion GENERATES real col_type entries, so an enum
% variant rel is a legal spread source once the two expansions are ordered.
enum_generated_columns(VariantName, Columns) :-
    Sugared = prog([ enum_decl(body, (page(view: text) ; redirect(to: text))) ],
                   []),
    expand_enum_program(Sugared, prog(ExpandedDecls, _)),
    declared_columns(ExpandedDecls, VariantName, Columns).
