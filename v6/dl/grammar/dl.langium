// dl.langium — MVP grammar subset for the v6 dl slice (plan M1). No cross-references
// (plain ID properties everywhere): the bridge resolves rel names itself against the
// decl tables, so no scope/linking services are needed. Scope is pinned in
// v6/plans/2026-07-24-v6-dl-mvp-slice.md and v6/dl/tasks.d.ts; anything not listed
// there (JSON5 terms, `=~`, arithmetic, string interpolation, latest()/interval!) is
// deliberately absent from this grammar (frontier, rejected at parse time by simply
// not existing as a production). Named args (`rel(col: term, ...)`) and `#` line
// comments landed 2026-07-24 (owner scope change, out of the frontier list above).

grammar Dl

entry Program:
    (statements+=Statement)*;

Statement:
    RelDecl | ShDecl | QueryStmt | DlRule;

// ── rel declarations ────────────────────────────────────────────────────────
// Surface: `rel name(cols...).` (retention "all"), `rel(0) name(cols...).`
// (retention 0 = scratch, dies with its tick), `rel(1) name(cols...).`
// (retention 1 = state, Key-upsert semantics — parses, semantics land later).

// `retention` is the INT terminal (validated as 0|1 in the bridge), NOT a keyword
// alternation: '0'/'1' as keywords would win over the INT terminal for EVERY
// occurrence of those digits anywhere in the language (same identifier-collision
// reasoning as PlainType.prim below) — an ordinary integer literal `0` or `1` in a
// rule body (`x = 0`) would then fail to lex as INT at all.
RelDecl:
    'rel' ('(' retention=INT ')')? name=ID
    '(' (columns+=ColumnDecl (',' columns+=ColumnDecl)*)? ')' '.';

ColumnDecl:
    name=ID ':' type=ColumnType;

ColumnType:
    PlainType | WrapperType;

// Key(text): parses, accepted, semantically inert this slice.
// Min(int)/Max(int): parse; USE is a load error ("frontier") — checked in the bridge.
//
// `prim` is a plain ID here (validated as "text"|"int" in the bridge), NOT a keyword
// alternation: a column can itself be NAMED `text` (the fixture does exactly this —
// `sh sg(... text: text)`), and a keyword's exact-match token always wins over the
// ID terminal REGARDLESS of grammar position — making "text"/"int" global keywords
// would make it impossible to ever name a column `text` anywhere in the language.
PlainType:
    prim=ID;

WrapperType:
    wrapper=('Key' | 'Min' | 'Max') '(' prim=ID ')';

// ── host decls ───────────────────────────────────────────────────────────────
// `sh name(cols...) = `template`.` — template body is the raw backtick string;
// inputCols are inferred later (bridge) by scanning the template for {col}/$col.

ShDecl:
    'sh' name=ID '(' (columns+=ColumnDecl (',' columns+=ColumnDecl)*)? ')' '='
    template=TEMPLATE '.';

// ── facts + rules ────────────────────────────────────────────────────────────
// A fact is a DlRule with no body: `head(args).` (args expected all-literal by
// convention; the bridge's literal-binding mint handles literal head args the
// same way it handles literal-binding comparisons). A rule has a body:
// `head <- item, item, ... .`

DlRule:
    head=HeadAtom ('<-' body+=BodyItem (',' body+=BodyItem)*)? '.';

HeadAtom:
    rel=ID '(' (args+=HeadArg (',' args+=HeadArg)*)? ')';

// Member first: Member and ArgTerm both start with ID, so the parser needs the
// 2-token lookahead (ID ':') to pick Member over a bare Var — putting it first in
// the alternative order resolves the ambiguity (verified: both `foo(path)` and
// `foo(path: x)` parse, see tests/0_ast_bridge.test.ts). AggCall is unambiguous
// against both (its second token is always '(').
HeadArg:
    Member | ArgTerm | AggCall;

// Head aggregates: `count(x)` / `sum(x)` / `min(x)` / `max(x)` in head position.
// `fn` is a plain ID (validated against the four names in the bridge), same
// same-word-as-identifier reasoning as PlainType.prim above.
AggCall:
    fn=ID '(' arg=Var ')';

// ── body items ───────────────────────────────────────────────────────────────
// Order matters only where alternatives share a first token; NegItem ('!' prefix)
// and CompareItem's literal-first form (a Literal token) are already unambiguous
// against the ID-first alternatives (RelRefItem/ProbeItem/MutationItem/CompareItem's
// var-first form), which are disambiguated by their SECOND token ('(' / '?' / '!' /
// a comparison operator).

BodyItem:
    NegItem | ProbeItem | MutationItem | CompareItem | RelRefItem;

RelRefItem:
    rel=ID '(' (args+=AtomArg (',' args+=AtomArg)*)? ')';

// Postfix probe: `name?(args)` — args split input/output cols by the host decl's
// inputCols in the bridge, not here (grammar doesn't know arity split). Named args
// resolve against the HOST decl's columns (same law as any other atom).
ProbeItem:
    rel=ID '?' '(' (args+=AtomArg (',' args+=AtomArg)*)? ')';

// Postfix mutation probe: `name!(args)` — parses; rejected at load
// ("mutations land with a later slice"). Named args parse here too (the bridge
// never inspects a mutation's args before rejecting it).
MutationItem:
    rel=ID '!' '(' (args+=AtomArg (',' args+=AtomArg)*)? ')';

// Prefix negation: `!rel(args)` (v5 surface spelling, no `not` keyword).
NegItem:
    '!' rel=ID '(' (args+=AtomArg (',' args+=AtomArg)*)? ')';

// Comparison: var-vs-literal in either order. Whether this becomes an ast.ts
// Compare or a minted literal-binding rel is a bridge-time decision (depends on
// whether the var is bound elsewhere in the body) — the grammar just captures
// both textual orders.
CompareItem:
    (lhs=Var op=('=' | '!=' | '<' | '<=' | '>' | '>=') rhs=Literal)
  | (lhs=Literal op=('=' | '!=' | '<' | '<=' | '>' | '>=') rhs=Var);

// ── query lines ──────────────────────────────────────────────────────────────

QueryStmt:
    '?' rel=ID '(' (args+=AtomArg (',' args+=AtomArg)*)? ')' '.';

// ── shared argument/term shapes ──────────────────────────────────────────────

// Named-arg call sites (positive body atoms, negation, probes, mutations, query
// lines): `Member` first for the same ID-prefix lookahead reason as HeadArg above.
AtomArg:
    Member | ArgTerm;

// One shared `ident: term` production, used by every named-arg call site above
// AND by HeadArg. FORWARD-COMPAT: this exact rule is reused verbatim as the future
// JSON5 object-literal member rule (DECISIONS.md "Types are tuples now; JSON5
// shapes later") — extend `MemberValue`, never add a second `ident: value` rule.
Member:
    key=ID ':' value=MemberValue;

// AggCall is only ever meaningful as a Member value in HEAD position (`hits:
// count(path)`); a body/probe/query atom naming an AggCall via Member parses (one
// shared production, no atom-specific carve-out) but is semantically inert there —
// no legitimate program writes it, and the bridge never produces one from that
// position.
MemberValue:
    ArgTerm | AggCall;

ArgTerm:
    Var | Literal | Wildcard;

Var:
    name=ID;

// The wildcard pitfall: `_` alone would lex as ID unless it is a keyword — a
// keyword whose exact text matches a terminal's pattern wins over that terminal
// in Langium's token ordering, so declaring `_` as a keyword here (instead of
// letting the ID terminal swallow it) makes positive-position `_` parse as
// Wildcard, not as a Var named "_". Verified in
// tests/0_ast_bridge.test.ts ("wildcard `_` parses as wildcard not ident").
Wildcard:
    {infer Wildcard} '_';

Literal:
    StrLit | IntLit | BoolLit | NullLit;

StrLit:
    value=STRING;

IntLit:
    value=INT;

BoolLit:
    raw=('true' | 'false');

NullLit:
    {infer NullLit} 'null';

// ── terminals ────────────────────────────────────────────────────────────────
// Idents: `[a-z_][a-z0-9_/]*` (slash-liberal, per the MVP scope). `#` line
// comments are v5 .dl heritage (every v5 .dl file in this repo uses `#`).

terminal ID: /[a-z_][a-z0-9_/]*/;
terminal INT returns number: /[0-9]+/;
terminal STRING: /"(\\.|[^"\\])*"/;
terminal TEMPLATE: /`[^`]*`/;

hidden terminal WS: /\s+/;
hidden terminal SL_COMMENT: /#[^\n\r]*/;
