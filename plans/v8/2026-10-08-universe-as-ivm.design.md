# Universe as IVM: entity liveness as a relation

Status: idea, user 2026-10-08 ("define universe as ivm ... only eval'ing these affected things bc
this new event is going to wake this subgraph of our universe"). Nothing built.

## Today (main, read 2026-10-08)

One string `"a"` lands in every store below; none frees in production.

| step | code | store | new entry |
|---|---|---|---|
| 1 | `Universe::string` | `Syms` (lasso arena or blob span) | `Sym` → bytes (copy 1) |
| 2 | `Universe::intern` `src/_6_eval/_0_term.rs:242` | `Universe.terms` | `TermId = Str(Sym)` |
| 3 | `install_terms` `src/_6_eval/_9b_ir_eval.rs:1359` → `intern_text` | `ivm-ir` `Interner.by_text` | `Cell` → `Arc<str>` (copy 2) |
| 4 | `install_terms` `:1426` → `intern_terms("2:str", [text])` | `Interner.by_key` | second `Cell` |
| 5 | same | `Core.mapped`, `Decoded.cells` `:79-130` | both directions |
| 6 | `drain_pending` → constructor input | DD source `2:str` | `(term, text) +1` |
| 7 | `bootstrap_changes` `:1448` | DD source `__ir_term_map` `_9a_ir_lower.rs:2998` | `(TermId, Cell) +1` |

`Universe::sweep` (`_0_term.rs:310`) exists; only tests call it.

## Data model (placeholder names)

```ts
type Term = { id: Cell; functor: string; args: Cell[] }
type Ref  = { id: Cell }
type Live = { id: Cell }
type Gone = { id: Cell }
```

## Rules

1. `ref`: per relation, per `Ty::Id` column, `Mfp project c`; `Union`.
2. `live = distinct(ref) ∪ children(live, term)`; a LetRec over term depth.
3. `gone = term antijoin live`; the host frees its +1 rows.

## Step trace

| step | event | wakes | live | gone |
|---|---|---|---|---|
| 1 | `+edge(C7,C8)` | edge readers, `ref` over edge | `+C7 +C8` | |
| 2 | mint `Pair(C7,C8)` = C9 | pair rule, `ref` over pair, child walk | `+C9` | |
| 3 | `+other(C42)` | other readers, `ref` over other only | `+C42` | |
| 4 | `−edge(C7,C8)` | edge readers → `pair −C9` → `ref` → live | `−C9 −C7 −C8` | `+C9 +C7 +C8` |
| 5 | host drains `gone` | | | consumed |

## Lifetimes

- A `Term` row lives from first mint until `gone` emits it.
- Content-hash cell ids (`hash(functor, args)`): a re-mint after free returns the same id, so no
  in-flight row can name reissued content.
- `TermId` and `Sym` collapse into `Cell`: one id space, one dictionary.

## To measure

- Operators and arrangements `ref` adds (one Mfp per Id column per relation).
- `live` recursion depth on the macrotime corpus.
- `gone` antijoin cost per settle versus the dictionary size.
