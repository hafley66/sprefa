% schema.pl : the ONE worked example this lab prices every option against.
%
% A route tree with an enum body and a SHARED subtree (the same view value
% hangs off two different routes). Everything the lab grades -- json
% round-trip, intern sharing, cascade, match-path joins, the three decl
% spellings -- runs on this schema and these values, so the numbers in the
% verdict are comparable across options.
%
% Vocabulary law: field/type words only from prolog (functor/arity/args),
% SQL (table, column, key), or json (object, array). No invented words.

:- module(schema,
          [ struct_type/2, enum_type/2, variant_of/4, field_spec/3,
            value_type/1, example_value/2 ]).

% ── struct types: TypeName, columns in DECLARED order ───────────────────────
% A variant table (body_page, body_redirect) is an ordinary struct type; the
% enum layer above it is pure naming (Q1 option (b): N variant tables sharing
% one id space).

struct_type(route,         [path, body, children]).
struct_type(view,          [title, tags]).
struct_type(body_page,     [view]).
struct_type(body_redirect, [to]).

% ── enum: name + the json discriminator field ───────────────────────────────
enum_type(body, kind).

% variant_of(Enum, Tag, VariantTable, Fields)
variant_of(body, page,     body_page,     [view]).
variant_of(body, redirect, body_redirect, [to]).

% ── field specs: Spec = text | int | struct(T) | enum(E) | list(Spec) ───────
field_spec(route,         path,     text).
field_spec(route,         body,     enum(body)).
field_spec(route,         children, list(struct(route))).
field_spec(view,          title,    text).
field_spec(view,          tags,     list(text)).
field_spec(body_page,     view,     struct(view)).
field_spec(body_redirect, to,       text).

value_type(text).
value_type(int).

% ── the model json values ───────────────────────────────────────────────────
% Model json term: obj(Pairs) | arr(Items) | str(Atom) | int(Number).
% Pairs are written in the order the printer must reproduce (json_text/2 is
% order-preserving, so "byte-identical" means exactly that).

% The full tree: one page route with two children, one of which repeats the
% SAME view value (title T, tags [x, y]) that the root carries.
example_value(route_tree,
    obj([ path-str('/a'),
          body-obj([ kind-str(page),
                     view-obj([ title-str('T'),
                                tags-arr([str(x), str(y)]) ]) ]),
          children-arr([
              obj([ path-str('/a/1'),
                    body-obj([ kind-str(redirect), to-str('/a') ]),
                    children-arr([]) ]),
              obj([ path-str('/a/2'),
                    body-obj([ kind-str(page),
                               view-obj([ title-str('T'),
                                          tags-arr([str(x), str(y)]) ]) ]),
                    children-arr([]) ]) ]) ])).

% The domination pair: two ROOTS sharing one view value. Releasing tree_a
% must leave the shared rows alive; releasing tree_b too must collect them.
example_value(tree_a,
    obj([ path-str('/a'),
          body-obj([ kind-str(page),
                     view-obj([ title-str('T'),
                                tags-arr([str(x), str(y)]) ]) ]),
          children-arr([]) ])).

example_value(tree_b,
    obj([ path-str('/b'),
          body-obj([ kind-str(page),
                     view-obj([ title-str('T'),
                                tags-arr([str(x), str(y)]) ]) ]),
          children-arr([]) ])).

% A redirect route, for the depth-2 match path and the enum-dispatch check.
example_value(tree_r,
    obj([ path-str('/r'),
          body-obj([ kind-str(redirect), to-str('/a') ]),
          children-arr([]) ])).

% Tail-sharing probe (cons vs indexed list pricing): two lists where one is
% the other with an element pushed on the FRONT.
example_value(tags_short, arr([str(x), str(y)])).
example_value(tags_long,  arr([str(w), str(x), str(y)])).
