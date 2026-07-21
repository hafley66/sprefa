# Symbol-dict normalization — correctness proof + plan correction

Date 2026-07-21. Supersedes the schema in `plans/2026-07-21-symbol-dict-normalization.md`.
Purpose: before one line of the multi-day sym arc is written, prove whether the
change is behavior-preserving, and catch — with data, not opinion — whether the
written plan is the wrong mark. All measurements are against the live root
`~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite` (opened `immutable=1`,
read-only), 2026-07-21.

## 1. The identity, formally

A symbol occurrence `a` has attributes `(repo, file, kind, name, parent, coord?)`.
`mint_sym`/`lambda_sym` (typegraph/mod.rs:412-434) build a string:

    enc(a) = "repo::file::kind::name"                      (free symbol)
           = "repo::file::kind::parent.name"               (method / scoped)
           = enc(enclosing) ++ "::closure::" ++ coord       (lambda)

The STORED identity is `id_old(a) = hash64(enc(a))` as i64 (`spine.rs:52`
`StringId::of`, `lower.rs:89` `sym_lit`). Every `sym`-typed rel column holds this
i64; every join is integer equality on it. **Therefore the join partition of the
corpus is exactly the partition induced by the string `enc(a)`.**

## 2. Theorem (join preservation)

Replacing `id_old` with any `id_new` preserves every equijoin result **iff**
`id_new` induces the same partition on the occurring occurrences as `id_old`:

    ∀ occurring a,b:   id_new(a) = id_new(b)  ⟺  enc(a) = enc(b)      (★)

*Proof.* Joins compare identities only for equality (never interpret them; display
is reconstructed separately). Two occurrences co-appear in an equijoin result iff
their identities are equal. If (★) holds, every pair equal under `id_new` is equal
under `id_old` and vice versa, so every equijoin yields the identical row set.
Conversely if (★) fails there exist `a,b` equal under one identity and not the
other, and the join that pairs them differs by exactly that pair. ∎

Given hash injectivity (measured §3.1), `id_old(a)=id_old(b) ⟺ enc(a)=enc(b)`, so
(★) reduces to: **`id_new` reproduces the partition-by-`enc`.**

## 3. Measurements (the receipts)

### 3.1 hash64 is collision-free here → string↔hash is a bijection
| rel | distinct sym ids | distinct sym STRINGS |
|---|---|---|
| type_entity | 6363 | 6363 |
| call_def | 11737 | 11737 |

Equal ⇒ no two distinct strings share a hash on this corpus. A surrogate keyed on
the full sym STRING is a perfect bijection with `id_old`.

### 3.2 enc is INJECTIVE on the corpus → the safe direction is proven
Check A — number of `sym` values that fold together **more than one** distinct
`(repo,file,kind,name,parent)` tuple: **0**.

⇒ No equijoin currently relies on a collision; no faithful re-identification can
wrongly SPLIT any currently-merged rows. (The change also removes the residual
64-bit hash-collision risk — strictly safer than today.)

### 3.3 the stored columns are a LOSSY decomposition of the sym (the landmine)
| measurement | value | consequence |
|---|---|---|
| distinct sym (type_entity) vs distinct 5-col tuple | 6363 vs **6362** | ≥1 tuple maps to 2 syms — the columns lose info the sym keeps |
| type_entity syms with scope-dot tail + EMPTY parent col | **5454** / 6363 | `parent` column does not capture the scope `enc` encodes |
| parent column non-empty | 1918 / 7372 | faithful for ~26% of rows only |
| call_def name/parent columns | **none exist** | 11737 distinct syms → only **1028** distinct `(repo,kind,file)` |
| closure syms (coord in no column) | **6018** | closure identity lives only in the string |

Worked landmine (the 6363-vs-6362 case): tuple
`(file=editors/vscode-dl/src/extension.ts, kind=const, name="fact", parent="")`
maps to **two** syms, `...::const::addMark.fact` and `...::const::addTypeSeed.fact`
— two distinct consts named `fact`, one inside `addMark()`, one inside
`addTypeSeed()`. `enc` distinguishes them (scope in the string); the `(name,parent)`
columns do not (both `name="fact", parent=""`).

## 4. Corollary — the written plan's schema is UNSOUND

`plans/2026-07-21-symbol-dict-normalization.md` keys `_sym_dict` on the stored
columns: `UNIQUE(repo, file, kind, name, parent)`. §3.3 proves those columns are
not a faithful decomposition of `enc`, so this key **merges distinct symbols**:

- **call_def**: no `name`/`parent` columns at all — keying on its columns collapses
  **11737 → ~1028** (every same-kind callable in a file becomes one symbol). A
  green suite would not catch it; the joins just quietly return the wrong sets.
- **type_entity**: the 5454 empty-parent scope-dotted syms collapse whenever two
  share `(file,kind,name)` — the `addMark.fact`/`addTypeSeed.fact` class, generalized.
- **closures**: 6018 syms whose `coord` is in no column — all closures in one
  enclosing fn merge.

This is exactly the "wrong mark, days rectifying" failure. **Do not implement the
plan's schema.**

## 5. The corrected design (provably correct)

**Root rule:** the surrogate is resolved at the **mint / extraction seam** from
`mint_sym`/`lambda_sym`'s TRUE arguments — the same values that build the string
today — and NEVER reconstructed from rel columns. This mirrors the df surrogate,
which resolves from each node's real `(file,line,col,kind)` coordinate at the
write seam, not from `rel_df_node` columns.

### Option A — string-interning surrogate (zero risk; does NOT clear the rail)
`_sym_dict: id ⟷ full sym string`. Provably a bijection (§3.1). Zero behavior
change, removes hash collisions. But `mint_sym` still builds the composite string,
so `composite-key-string.dl` still fires. Use only if the goal is "dense ids, no
hash", not "no `format!`".

### Option B — structured surrogate (clears the rail; correct ONLY with a faithful key)
`mint` returns a structured key; the write seam interns it to a dense surrogate.
The key MUST carry every discriminator `enc` uses:

    SymKey = (repo, file, kind, name,
              enclosing: Option<SymSurrogate>,   // recursive scope, NOT a flat string parent
              coord:     Option<Coord>)          // closure position

- `enclosing` is the surrogate of the enclosing callable (captures `fact`-inside-
  `addMark`, and nesting to any depth), resolved before the inner symbol.
- `coord` is the closure node coordinate for lambdas.
- The plan's flat `(repo,file,kind,name,parent)` is INSUFFICIENT — it drops `coord`
  (6018 closures) and flattens scope to one level. Proven, not assumed.

`_sym_dict` DDL then: `UNIQUE(repo, file, kind, name, enclosing, coord)` with
`enclosing`/`coord` nullable, and `sym_decode` re-renders `enc` for display only.

## 6. The gate that replaces trust (mechanical proof)

The migration MUST run, as a build-time assertion over the FULL corpus:

**Bijection check** — per sym-bearing rel: `count(distinct new surrogate) ==
count(distinct old sym hash)` (baselines today: type_entity 6363, call_def 11737,
plus every other sym rel). Equal ⇒ new identity is a bijection with old on this
corpus ⇒ behavior-preserving, PROVEN. Not equal ⇒ the surrogate merged or split
something ⇒ **HALT and dump the delta tuples** for adjudication. A silent merge
passes a green suite; this is the only thing that catches it.

**Join-parity probe** — closes the atomicity / silent-empty-join risk. Because
join = equality on the surrogate, a column left as a hash while its join partner
became a surrogate puts them in disjoint integer spaces and the join returns ∅.
For every cross-family sym join — `df_node.fn_sym ↔ call_def.sym`, closure
`df_node.var ↔ call_def.sym`, `type_edge.from/to ↔ type_entity.sym`,
`call_edge.caller/callee ↔ call_def.sym`, ` type_link.src/dst`, ... — assert
`rowcount(new) == rowcount(old)`. Enumerate the sym columns statically (invariant:
*every* sym column resolves through `_sym_dict`) and pin one probe per join.

## 7. Verdict

- The idea — dict surrogate replacing the folded hash — is correct, and the safe
  direction is PROVEN (Check A = 0; bijection baseline exact).
- The written plan's schema is a landmine: keyed on lossy stored columns, it would
  silently merge symbols (catastrophically for call_def). Reject it.
- Bulletproof path = Option B with: mint-seam resolution, a full-discriminator key
  (`enclosing` surrogate + `coord`), a build-time bijection gate that HALTS on any
  delta, and per-join parity probes. Anything less repeats the wrong mark.
