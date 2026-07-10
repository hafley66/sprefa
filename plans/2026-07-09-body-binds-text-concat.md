# S3 body-level binds + S4 text concat

Ledgered agent complaints (CLAUDE.md batch 2, S3/S4). Triage against code FIRST
changed the shape of both.

## Triage findings

**S3 mostly exists.** `body_sql` (lower.rs:160-186) already has the
computed-binding path: a Cmp `var = expr` where `var` is unbound and the other
side is Call/Arith (`has_computation`) inserts `var -> expr SQL` into canon.
The exact target surface (`callee = replace(callee_q, ".", "::")` in a derived
body, later use in the head) runs green today; documented in syntax.md's strfn
row. What the complaining agent hit was a SOURCE rule body: `val_of`
(engine/mod.rs:6997) errors `unbound var {v} in constraint` + `note: to compute
a new value, put the expression in the rule head: head(path, line+1) <- ...` —
that IS the "good error message" from the complaint. Gaps to close:
  1. Boundness diagnostic at TYPECHECK time for a bind whose RHS var is bound
     nowhere (today a lower-time `unbound variable {v}` abort, no fix named).
  2. A bind var used in a NEGATION is silently wrong: body_sql runs the Neg
     pass BEFORE the Cmp pass, so the bind var is absent from canon and becomes
     a LOCAL of the NOT EXISTS subquery (unconstrained). Reorder Cmp before Neg.
  3. Source-rule refusal message: extend the val_of note to say body binds work
     in derived rules (no second evaluator — per scope).
  4. The old ?-then-ref intra-row self-eq machinery (`__rule.` desugar,
     294d60db) is GONE post-lift (zero grep hits); the compat requirement
     reduces to: bare `x = y` (Var=Var) stays a WHERE filter — `has_computation`
     already keeps the forms apart (only Call/Arith RHS binds).

**S4 is a silent-wrong-answer today.** `+` parses as `Term::Arith{Add}` and
lowers to SQLite `+` unconditionally: `"https://" + host` returns `0` (numeric
coercion), no error. Source rules (`val_of`) DO error ("arithmetic needs int
operands"). Fork taken: OVERLOAD `+` — no grammar ambiguity exists (`+` already
parses in head + comparison positions); the cost is type-directed lowering,
which the rel metadata already funds (Col.ty is the base storage type).

## Type signatures

    // lower.rs
    fn term_sql(t: &Term, canon: &HashMap<String,String>, tys: &HashMap<String,Type>) -> Result<String>
    fn term_ty(t: &Term, tys: &HashMap<String,Type>) -> Option<Type>   // None = unknown
    fn body_sql(body, rels) -> Result<(HashMap<String,String>, HashMap<String,Type>, Vec<String>, Vec<String>)>
        // canon, tys, froms, wheres — tys filled at every canon insertion:
        // atom var -> meta.cols[pos].ty, bind var -> term_ty(rhs)

    // typecheck.rs
    fn arith_ty(t: &Term, seen: &mut HashMap<String,ColTy>, ...) -> Option<Type>
        // bottom-up; Add polymorphic (int+int / text+text, mixed -> diag
        // `plus-mismatch` naming the interp/int() fix); Sub/Mul/Div/Mod int-only
    // check_rule_types: new body-order Cmp walk (derived-shaped bodies only):
    //   bind detection mirrors lower (target ∉ atom vars ∪ earlier binds,
    //   has_computation(rhs)); RHS var not in atom vars ∪ earlier binds ->
    //   error `unbound-bind`: "bind `{rhs_var}` before computing `{target}`"

    // engine val_of: Add on (Text, Text) -> concatenation; mixed -> error
    // naming the fix; unbound-var note gains the derived-rule bind sentence.

## Pseudo-code

    term_sql Arith arm:
      Add => match (term_ty(lhs), term_ty(rhs)):
        (Int, Int)                        -> "(l + r)"
        (text-base, text-base)            -> "(l || r)"     // path/file/... store TEXT
        (Some(Int), Some(text)) or rev    -> bail "cannot `+` int and text — ..."
        (None, other) | (other, None)     -> other==Int or None -> "+" (legacy), text -> "||"
      Sub/Mul/Div/Mod => "(l op r)" unchanged

    body_sql pass order: Pos -> Cmp (binds + filters) -> Neg
      (was Pos -> Neg -> Cmp; a bind var referenced in a Neg atom now joins
       correctly instead of minting an unconstrained subquery local)

    check_rule_types additions:
      atom Arith arm -> arith_ty replaces the force-int walk; fill-column check
        by inferred type (Int -> int col, Text -> text-base col)
      derived-shape Cmp walk in body order:
        bind -> boundness check + seen.insert(target, rhs ty) + arith_ty(rhs)
        filter -> arith_ty both sides (catches mixed + in filters)

## Instance lifetimes

- `tys` lives exactly as long as `canon` (one lowering call), dropped after.
- typecheck `seen`/bind sets: per-rule, dropped after check_rule_types.
- No engine/db state; storage untouched (TEXT/INTEGER as today).

## Storage / reads / writes

None. Parse unchanged (both surfaces already parse). Reads: rel col types from
the same Rels metadata lowering already holds. Writes: TypeDiags only
(`unbound-bind`, `plus-mismatch`). SQL emitted changes ONLY for `+` over text
operands (`||`) and for the Cmp/Neg pass order (a strictly-more-correct join).

## Uniqueness / compat

- Bind vs filter vs equality: `x = fn(..)` binds iff x unbound; bound-x =
  fn(..) stays a filter; Var=Var stays a filter (has_computation gate, unchanged).
- Atom vars are ORDER-FREE for boundness (SQL joins are declarative; lower's
  canon has every Pos var before the Cmp pass) — only bind->bind chains are
  ordered. The diagnostic mirrors the evaluator exactly; stricter
  earlier-item-only ordering would reject programs that run today.
- Source rules keep the refusal (no second evaluator), message now names the
  derived-rule alternative.

## Shipped (implementation notes)

Lowered SQL for the canonical bind (from `lower_rule`, pinned by the
`bind_lowers_to_inlined_expr_sql` unit test — the expr inlines into the head
SELECT, no subquery, one evaluator):

    INSERT OR IGNORE INTO rel_resolved ("caller", "callee")
    SELECT r0."caller", replace(r0."callee_q", '.', '::')
    FROM rel_raw_edge r0

Text `+`: `endpoint("https://" + host)` lowers to `'https://' || r0."host"`;
`next_line(line + 1)` stays `r0."line" + 1`.

Deviation from the brief: bind-RHS boundness is atom-order-free (any positive
body atom binds, only bind->bind chains are position-ordered) because that is
what the evaluator does — canon holds every Pos var before the Cmp pass, and a
stricter earlier-item-only rule would reject programs that run today.
