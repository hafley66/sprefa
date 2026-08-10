# Generics and list recon

Date: 2026-08-10. Scope: enumeration and pricing only. Every unresolved semantic choice appears as a fork.

## Receipts and current facts

| fact | receipt |
|---|---|
| `option(T)` and `T?` parse to the same term; each element type gets one enum | `v6/prolog/compile/parse_dl.pl:688-705`; `v6/prolog/conformance/rulings.pl:632-634` |
| The removed `list(T)` spelling records a finding; `json_list(T)` currently parses. Recursive `typed_column_type/3` admits nesting syntactically. | `v6/prolog/compile/parse_dl.pl:690-724` |
| Current list elements are scalar/json/nested-list values. A relation element throws `unsupported_construct(list_of_relation_refs(Element))`. | `v6/prolog/0_type_plane.pl:115-140` |
| Current physical list carrier is a JSON storage kind. Array and element guards are described but not emitted. | `v6/prolog/0_type_plane.pl:108-115` |
| The relation-reference throw exists because tick logs print values rather than dictionary ids. | `v6/prolog/0_type_plane.pl:130-134` |
| Option expansion is 122 lines, recursively rewrites declarations, and leaves rules unchanged. | `v6/prolog/0_option_expand.pl:15-25`; `wc -l v6/prolog/0_option_expand.pl` = 122 |
| Compilation performs host preparation, ordered expansion, reference-target materialization, catalog materialization, checks, then planning. | `v6/prolog/compile.pl:155-230` |
| Expansion order is option 5, enum 10, match 40, sequence 42, dot 44, coalesce 45, AST 46, negated guard 47, relation-edge 50. | `v6/prolog/1_expansion.pl:26-62` |
| Arrival targets are all referenced, declared, or seeded relations minus compiler catalog and derived heads. A minted relation can therefore be a body read or rule head; heading it removes it from arrival targets. | `v6/prolog/compile.pl:182-198` |
| Existing option enum names use `__opt_<element>` without escaping or collision detection. | `v6/prolog/0_option_expand.pl:66-78` |
| Stored relations use integer surrogate identities; natural keys live once in a dictionary; hot relation columns carry ids. | `.claude/skills/sql-relational-design/SKILL.md`, “The law” and “The rest of day-1” |
| INTEGER-keyed four-column `WITHOUT ROWID` inserts measured 1.7–2.0x faster than TEXT; indexes copy keys. | `.claude/skills/sqlite-costs/SKILL.md`, “Write rates” and “Facts that veto common proposals” |
| Parenthesized comptime parameters are the recorded generic surface direction. | `v6/prolog/conformance/rulings.pl:695-702`, commit `a5570ced` |

The 2026-08-10 session direction supplied with this recon splits `list(T)` from `json_list(T)`: `list(T)` denotes a relational child collection, while `json_list(T)` remains an inline carrier. It also supplies one innermost-first fixpoint expansion pass and a canonical type-name function as the mechanism to price. These are session inputs, not rows in `conformance/rulings.pl`.

## 1. List edge-case inventory

The relational candidate used below has a list dictionary `L(list_id INTEGER PRIMARY KEY, ...)` and members `M(list_id INTEGER, idx INTEGER, value_id/value, PRIMARY KEY(list_id, idx)) WITHOUT ROWID`. Natural content identity, where selected, adds a canonical-content dictionary rather than putting JSON or TEXT into a hot key. This follows the two mandatory repository skills cited above.

| case | engine collision | semantic/storage forks and price |
|---|---|---|
| `[]` versus absence | Option expansion represents per-instance absence; a child relation with zero members otherwise leaves no row proving an empty list exists (`0_option_expand.pl:66-75`, `:115-122`). | **A:** allocate a list entity even at length zero: one dictionary row, distinguishable from absent. **B:** parent-to-list companion row denotes presence and zero members denotes empty: one parent link, list entity may be elided for unshared lists. **C:** conflate empty with absence: zero rows, loses `some([])` versus `none`. |
| Empty-list arrival | World-shape validation runs before planning (`compile.pl:174-181`); no member arrival can create an empty collection. | **A:** explicit list/entity arrival event. **B:** explicit parent-list link arrival. **C:** reserved zero-member constructor syntax lowered before the arrival gate. Each adds one independently validated ingress shape. |
| Duplicates | A member key `(list_id, idx)` preserves equal values at distinct positions; `(list_id, value)` erases multiplicity. | **A:** sequence/multiset duplicates retained by index. **B:** set flavor dedups by value. Checker must reject duplicate-sensitive operations on B or define their set result. |
| Ordering | SQL row order is absent without `ORDER BY`; `json_group_array` must consume a preordered input. | **A:** dense integer `idx`. **B:** sparse/fractional order key. **C:** linked predecessor/successor ids. All rendering and equality reads require explicit order. |
| Swap delta | Dense `(list_id, idx, value)` makes a swap two retracts plus two asserts. | **A:** positional identity: 2+2 row changes. **B:** stable member id plus mutable order: two order-field changes, still 2+2 in differential storage. **C:** linked order: neighbor rewrites scale with touched adjacency. |
| Insert-middle storm | Dense indices shift the suffix, producing O(n) retract/assert pairs; every keyed write performs btree work described by `sqlite-costs`. | **A:** dense, O(n) shifted rows. **B:** gapped integers, usually one insert and occasional O(n) rebalance. **C:** fractional/lexicographic labels, one insert until label exhaustion, wider keys. **D:** linked order, one member plus adjacency changes, ordered scans need traversal/recursive query. |
| Index stability across ticks | Recomputed dense indices identify positions, not members. Equal values also defeat value-derived member identity. | **A:** `idx` is identity and may change. **B:** append-only `member_id` plus order key; one extra integer per row and dictionary allocation. **C:** content-addressed occurrence plus ordinal; deterministic only after duplicate occurrence matching is specified. |
| Nested `list(list(T))` | Parser and current type predicate recurse (`parse_dl.pl:701-724`; `0_type_plane.pl:135-140`). Relational lowering needs members that reference list entities. | **A:** member `value_list_id`, one dictionary hop per level. **B:** distinct minted member relation per nesting level. Checker rejects infinite/cyclic type expansion and runtime list cycles if values are trees. |
| `option(list(T))` | Reference option drops the parent column and mints a companion keyed by parent (`0_option_expand.pl:80-122`). | **A:** option expansion sees minted list entity as a relation ref, companion presence distinguishes none/empty. **B:** list template owns nullable parent link. Pass order must guarantee exactly one owner. |
| `list(option(T))` | Scalar option currently becomes enum ids (`0_option_expand.pl:66-78`), while list members need a uniform value carrier. | **A:** member value is `__opt_T` dictionary id. **B:** member has presence plus value columns/companion. A costs a dictionary join; B widens list member templates and uniqueness rules. |
| List in a KEY column | Atomic-column law excludes JSON/TEXT pretending to be a key; current option explicitly throws in key columns (`0_option_expand.pl:31-35`). | **A:** list entity id participates in key, identity equality. **B:** intern canonical content and key by content id, content equality. **C:** named `unsupported_construct(list_in_key_column(...))`. A permits equal-content distinct keys; B pays canonicalization/dedup. |
| Equality | Separate entity ids may contain identical ordered members. | **A:** identity equality by `list_id`, O(1) probe. **B:** ordered content equality through canonical content id, canonicalize on construction. **C:** structural join on `(idx,value)`, O(n) plus length check. |
| Content identity and interning | Repository storage law places natural identity once in a dictionary. JSON/TEXT content cannot be the hot key. | **A:** global canonical list dictionary, content dedup across parents. **B:** per-element-type dictionary. **C:** no content interning, only entity identity. A needs type in the dictionary identity; B creates more tables; C duplicates member storage. |
| List of relation refs | Current code throws because raw ids would leak into the tick log (`0_type_plane.pl:119-120`, `:130-134`). | **A:** store child dictionary id and join back to printable child values at log boundary. **B:** store a list-member entity whose value columns copy printable natural values, violating the single-natural-key rule unless explicitly waived. **C:** retain the cited throw. A adds one join per reference layer. |
| Tick-log dictionary join | Interned text already requires boundary decoding; list refs add list and element dictionaries. | **A:** expand a list value into one ordered JSON array in the log query. **B:** emit member events separately. A preserves one logical value but performs ordered aggregation; B changes log cardinality and retraction representation. |
| Retraction cascade | Deleting one list entity may touch its members and parent links. SQLite keyed writes dominate measured tail cost (`sqlite-costs`, “engine-level decomposition”). | **A:** reference counts, delete members only at zero: increment/decrement writes plus last-owner cascade. **B:** append-only dictionary/members, retract links only: storage grows. **C:** ownership-only list, direct cascade O(n), forbids sharing. |
| Shared list: one-to-many versus many-to-many | A list column on a parent is one parent-to-list edge; equal or intentionally shared lists introduce multiple parents. | **A:** list entity with parent-list junction, many-to-many and refcount/liveness. **B:** list owned by one parent, one-to-many members and copied shared content. **C:** canonical content entity shared automatically. |
| Aggregate INTO list | Aggregate heads are typed during the program-wide type fixpoint (`compile.pl:199-210`); SQLite aggregation order must be explicit. | **A:** aggregate emits ordered child rows then interns/links a list. **B:** aggregate emits `json_group_array` carrier then a guard/import stage builds the child relation. **C:** `json_list(T)` only for aggregate heads. A needs a multi-relation maintained head; B crosses JSON and relation representations. |
| OUT via unnest/explode | Rule bodies currently read ordinary relations; option expansion expects authors to consume desugared relations (`0_option_expand.pl:15-18`). | **A:** compiler rewrites `each(List,Idx,X)` to member relation. **B:** minted member relation is named and read directly. **C:** library rule exposes it. A adds body sugar; B exposes generated names; C adds opt-in declarations/rules. |
| Derived list in a rule HEAD | Heading a minted member/list relation makes it derived and removes it from arrival targets (`compile.pl:193-198`). The supplied 2026-08-08 oracle finding says wiring the same derived relation as an arrival target silently duplicates rows, violating the one-relation/one-rule-kind operational law. | **A:** one logical list head expands atomically into entity, members, and link heads. **B:** head may target only an explicit list-builder relation, followed by template rules. **C:** named error for list-valued heads. A/B must ensure no generated derived relation is also wired for arrivals. |
| BODY read of minted relations | `program_refs/2` includes body refs before tables and plans are made (`compile.pl:188-219`). | **A:** direct generated-name body atoms. **B:** list surface accessor rewrites later. **C:** library accessor rules. Name hygiene and expansion order differ; all are mechanically legal after expansion. |
| `log` plus list column | `kind(log)` survives option parent arity rewriting (`0_option_expand.pl:98-101`); list membership introduces several physical relations under one logical log row. | **A:** log parent-link changes and render whole list. **B:** log each member delta. **C:** forbid relational lists on log rels at checker. A can reconstruct both old/new values only if member lifetime covers rendering. |
| `keep(...)` plus list column | Retention applies to a relation declaration; companion/member lifetimes are otherwise unspecified. | **A:** propagate keep policy to link and owned members. **B:** retain links, GC members by reference count. **C:** append-only list dictionary. The template must state which declarations inherit `keep`. |
| Canonical minted names across fixtures/modules | `__opt_<t>` is raw concatenation (`0_option_expand.pl:77-78`); modules and nested arguments create collision and spelling hazards. | **A:** length-prefixed structural encoding. **B:** readable escaped encoding plus stable hash suffix. **C:** module-local counter, which fails cross-fixture determinism. A/B are deterministic functions of the normalized type AST. |
| User-name collision | Reserved namespace check runs on author text before expansion (`compile.pl:159-165`), but today option names can collide after that. | **A:** reserve the complete generated prefix and reject author declarations. **B:** collision-check generated declarations against all author/generated names. **C:** hash namespace scoped by template identity. Named error must include both constructors. |
| Type aliases and spelling identity | `T?` and `option(T)` normalize to one term (`parse_dl.pl:688-705`). Future aliases could produce multiple spellings for one type. | **A:** canonical name from normalized type AST after alias resolution. **B:** spelling-derived names. B permits equivalent types to mint different artifacts. |
| Construction completeness | Entity/list/member rows can arrive over separate events, exposing a partially built list. | **A:** transaction-scoped constructor commits all rows together. **B:** completeness/length row gates visibility. **C:** list is visible incrementally. A needs multi-relation atomic lowering; B adds maintained state; C defines intermediate tick values. |
| Concurrent writers | Two producers can choose the same dense index or independently intern equal contents. | **A:** constructors are sole writers. **B:** writer id participates in occurrence identity then deterministic merge order resolves. **C:** constraint failure is surfaced. |
| Cycles in list values | Relational list ids permit self or mutual reference once nested lists exist. | **A:** tree-only checker/runtime guard. **B:** cyclic values permitted; equality, rendering, and GC need visited sets/fixpoints. **C:** nested lists stored inline at the leaf boundary. |
| Element type changes/migration | A canonical name encodes element type, so `list(int)` and `list(text)` are separate schemas. | **A:** migrate by derive-new-list plus relink. **B:** one dynamically tagged member table. B adds tag checks and weaker static columns. |
| Order-integrity gaps | Sparse/deleted indices may contain gaps; duplicates may violate a dense invariant. | **A:** gaps legal, only total order and uniqueness required. **B:** maintained check relation reports non-dense sequences. **C:** normalize on every change. B adds rules; C can cause suffix storms. |
| Length | `COUNT(*)` is derivable from members but repeated consumers repeat work. | **A:** query-time count. **B:** maintained `list_length(list_id,n)` rule. **C:** stored counter updated with membership. B joins maintained state; C introduces write coordination. |
| Slicing/concatenation | Sharing permits views into existing members; ownership permits copies only. | **A:** materialize new list members. **B:** list segments reference source list/ranges. B makes equality, GC, and rendering recursive. |

## 2. Do generic templates prescribe rules?

### Present legality and behavior

A generated relation name has no special restriction after expansion. A BODY atom is collected by `program_refs/2`; a HEAD atom is collected by `derived_refs/2` and excluded from arrivals (`compile.pl:182-219`). Option currently mints declarations only and explicitly preserves the original rule list (`0_option_expand.pl:15-18`). Its scalar enum and reference companion therefore can be read or headed using their generated names. Other checker constraints still apply, including the existing keyed-level-head path documented by `plans/2026-07-30-option-versus-null-lab.md:141-151`.

The supplied oracle receipt from 2026-08-08 adds an operational constraint: a derived relation also wired as an arrival target silently duplicates rows. Any rule-minting template must keep every generated relation in exactly one writer category. The compiler's current subtraction at `compile.pl:193-198` already expresses that partition for the emitted plan; template-generated ingress or host wiring has to use the post-expansion partition too.

### Artifacts a relational list may maintain

| artifact | declaration-only form | template-prescribed rule form | user-opted library-rule form |
|---|---|---|---|
| order integrity | member declaration carries unique order key; checker validates static schema only | template mints violation/check relation and rule over duplicate/gapped order | user imports a validator and chooses dense/gapped policy |
| length | consumers aggregate members | template mints `list_length/2` declaration plus maintained aggregate rule | user imports length rules only where queried |
| JSON view | boundary renderer performs ordered aggregation | template mints view declaration plus rule using ordered `json_group_array` | user imports a JSON-view rule and its naming contract |
| liveness/refcount | append-only or external GC contract | template mints owner-count and collectible relations/rules | user imports GC policy |

| mechanism fork | implementation surface | recurring engine work | failure surface |
|---|---|---|---|
| **A. declarations + rules + guards** | Template rows may emit declarations, rules, and checker guards. | Every instantiation expands, stratifies, types, subscribes, lowers, and stores the maintained artifacts. Each extra keyed head adds keyed writes; indexes copy their keys per `sqlite-costs`. | Cycles among generated rules, mixed writer category, later-name dependency, duplicate generated rule, guard ordering. |
| **B. declarations only** | Template rows describe entity/member/link schemas and associated metadata. | Consumers pay query-time aggregate/order work; no automatic maintained heads. | Missing library capability appears at use site; generated schema still needs collision and key checks. |
| **C. declarations plus opt-in library rules** | Core template emits schema; named library modules emit selected artifacts. | Cost exists only for imported artifacts; each library requires compatible canonical names or a typed handle. | Import duplication, version/schema mismatch, multiple libraries heading one artifact. |

### Pipeline placement fork

Current order is host preparation, option, enum, match, sequence, dot, coalesce, AST, negated guards, relation edges, materialization, checks, planning (`compile.pl:162-175`; `1_expansion.pl:26-62`).

| placement | ordering contract | price when a minted rule references a later-minted name |
|---|---|---|
| **A. one generic fixpoint before enum, replacing option phase 5** | Normalize type AST, expand innermost instantiations to a fixpoint, then enum sees all generated enums; later rule sugar still expands normally. | No generic name is “later”: all generic constructors close before rule-sugar phases. A template rule may reference match/dot/coalesce surface and those later phases rewrite it. |
| **B. one generic fixpoint after enum, before match** | Existing enum expansion cannot see enums minted by generic templates unless enum is folded into the same fixpoint or rerun. | A generated option enum or enum consumer escapes enum expansion. Requires generic+enum mutual fixpoint or a second enum pass. |
| **C. per-template phases distributed through the pipeline** | Each template declares dependencies and rank. | A rule referencing a later artifact can be typed or transformed before its declaration exists; forward references may work in `program_refs`, while enum context is frozen at `1_expansion.pl:77-89`. Requires dependency sorting, cycle diagnostics, and context recomputation. |

## 3. Surface fork: options versus named flavors

The comparison concerns API shape. Zig exposes fully parameterized constructors and convenience constructors in its standard namespace, including `HashMap`/`AutoHashMap` and `ArrayListAligned`/`ArrayList` ([Zig standard library 0.16](https://ziglang.org/documentation/0.16.0/std/)). Rust exposes sequence structures as named types, including `Vec`, `VecDeque`, and `LinkedList` ([Rust collections](https://doc.rust-lang.org/std/collections/)). The repository surface constraint is comptime parentheses (`conformance/rulings.pl:695-702`).

Potential second-argument fields are compile-time schema choices:

| field | candidate values | canonical-name consequence | checker work |
|---|---|---|---|
| order scheme | `dense`, `gapped`, `linked`, `unordered` | `list(text,dense)` and `list(text,gapped)` mint different member schemas and names | reject ordered operations on unordered; validate required key columns and integrity policy |
| identity/storage | `owned`, `entity`, `interned_content` | changes parent link, sharing, equality, and GC artifacts, so it must enter the name | reject identity equality where only content identity exists; reject sharing under owned |
| capacity/bound | `unbounded`, integer maximum | a hard bound changes guards/schema contract and enters the name; a performance hint does not define a type and stays out | integer, positive/nonnegative convention, nesting product/limit, arrival and aggregate overflow behavior |
| duplicate policy | `sequence`, `set`, `bag` | changes keys and equality, therefore enters name | reject duplicate-sensitive indexing for set; specify bag order |
| integrity artifact set | none/length/json/check flags | if artifacts alter the public generated relation set, either encode flags in names or place artifacts under a separate capability name | reject conflicting writers and duplicate capabilities |

| road | spelling examples | canonical namer | initial implementation price | migration if the first pick changes |
|---|---|---|---|---|
| **A. parameterized template plus named default wrapper** | `list(text)` expands as a wrapper around `list_with(text, dense, entity, unbounded, sequence)`; explicit form uses comptime parentheses | Canonicalize defaults before naming, so wrapper and fully explicit default share one name. Encode constructor id, normalized args, lengths/escapes, and stable schema version. | Parse heterogeneous comptime arguments; default filling; field validation; one underlying template table. | A→B: mint aliases/named wrappers for observed option tuples; old explicit forms can remain as compatibility spellings. Public signatures containing option tuples need rewriting if removed. |
| **B. named flavors** | `list(text)`, `deque(text)`, `linked_list(text)`, `interned_list(text)` | Constructor name is part of canonical AST; each constructor owns a fixed schema/options row. | More template names and rows; parser only needs type arguments already represented by compound terms. | B→A: map each old constructor to an option tuple wrapper. Name preservation needs aliases or migration of stored schema/catalog identities. |
| **C. one stock `list(T)` now, flavors later** | only `list(text)` initially | Canonical name must still include constructor and normalized element recursively; leave a version/component boundary for later constructors. | One template row and no options grammar. Checker names every unimplemented alternative operation explicitly. | Adding A later preserves `list(T)` as default wrapper if its first storage contract remains the default. Changing that default requires schema migration or versioned canonical names. Adding B later leaves existing name intact. |

Canonical-name checker obligations shared by all roads: reject unknown option keys/values, duplicate keys, non-comptime arguments, illegal combinations, recursive constructor cycles, reserved/generated name collisions, two normalized types mapping to one name, and equivalent normalized types mapping to different names. Cross-fixture determinism requires no declaration-order counter, process hash seed, path, or module-load order in the name.

## 4. Price of the general mechanism

### Proposed signatures, timelines, and storage

```prolog
canonical_type_name(+NormalizedType, -Atom).
normalize_type(+SurfaceType, -NormalizedType).
template_instance(+NormalizedType, -Decls, -Rules, -Guards, -Dependencies).
expand_generic_program(+Program0, -Program).
```

One compile owns one expansion state. It scans declarations and generated artifacts for generic type terms, normalizes each term, expands innermost dependencies, memoizes by normalized term, and repeats until the worklist is empty. The state dies before enum/match/dot phases. Generated declarations/rules are appended in canonical-name order, not discovery order, so fixture order does not change output.

Storage during expansion is a map `NormalizedType -> pending | expanded(GeneratedNames)` plus a set of all author and generated relation/type names. Reads: parse/normalize constructor, lookup template, compute dependencies, lookup collision set. Writes: mark pending, enqueue dependencies, emit artifacts after dependencies are expanded, mark expanded. Uniqueness is by normalized structural type. A second request for the same type reads the memo and emits nothing.

### Template table

| constructor | normalized arguments | generated declarations | generated rules/guards | present bespoke solve |
|---|---|---|---|---|
| `enum(Name,Variants)` | declaration name plus normalized variant field types | variant relations, keys, type metadata | declaration checks; no generic rules required | `v6/prolog/0_enum_expand.pl`; runs phase 10 |
| `option(T)` | recursively normalized `T` | scalar: `__opt_T` enum; relation: parent companion and rewritten parent declaration | currently none; key/unknown/enum checks throw at `0_option_expand.pl:27-45` | 122-line `0_option_expand.pl`; phase 5 |
| `list(T)` | recursively normalized `T`, plus selected flavor/options | list entity, member, parent link, optional capability declarations | fork in §2: none, all maintained artifacts, or imported capabilities | current `0_type_plane.pl:115-140` only classifies a JSON-backed storage kind; relational expansion absent |

### Size estimate

| component | estimated Prolog LOC | basis |
|---|---:|---|
| normalized type AST + canonical name encoding | 70–120 | recursive constructors, escaping/length prefix, schema version, collision errors |
| worklist/fixpoint/memo and deterministic emission | 70–110 | replaces `expand_option_decls/2` recursion at `0_option_expand.pl:20-25` with multi-template dependencies |
| template table interpreter | 50–90 | common declaration/rule/guard artifact vocabulary |
| enum adapter | 35–70 | maps existing variant generation into table rows/context |
| option adapter | 55–90 | scalar enum and relation companion split retain current branches/checks |
| initial relational list template | 80–150 | entity/member/link declarations; upper end includes selected guards, excludes optional maintained rules |
| diagnostics and collision checks | 45–80 | named unknown constructor/argument, cycles, name collisions, unsupported combinations |
| **mechanism plus three adapters** | **405–710** | **3.3–5.8 times the measured 122-line option module** |

LOC is a planning range. It excludes parser changes, lowerer/renderer changes for new relation shapes, emitted runtime code, migrations, and tests.

### Termination

Termination follows if all conditions hold: the template constructor set is finite; each template emits a finite artifact set; every dependency is a finite normalized ground type; recursive dependencies descend structurally or are rejected when an identical type is already `pending`; and expansion is memoized by normalized type. Innermost-first processing makes `list(option(text))` expand `option(text)` before its list artifacts. A template that synthesizes a strictly larger dependency such as `T -> list(T)` trips the pending/cycle or non-descending-dependency guard instead of extending the worklist indefinitely.

### Test surface

| group | cases |
|---|---|
| normalization/name goldens | `option(text)` = `text?`; whitespace independence; `list(option(text))`; `option(list(text))`; `list(list(text))`; explicit defaults = wrapper default; module/import order permutation |
| collision goldens | user declaration equals generated name; escaped atoms that would collide under concatenation; nested boundary ambiguity; same short hash with distinct full encodings; schema-version difference |
| fixpoint goldens | repeated instance emits once; two parents share an instance; dependencies emitted before consumers; output order independent of declaration order; enum minted by option available to enum phase |
| rule goldens | generated body read; generated derived head; generated match/dot/coalesce rewritten by later phases; generated rule never registered as arrival target; template dependency cycle |
| list semantics | every row in §1, including none/empty, duplicates, reorder deltas, middle insertion, sharing, retraction, nesting, key usage, aggregate in/out, log/keep, and cross-fixture names |
| named errors | unknown template, wrong arity, non-ground/non-comptime arg, unknown/duplicate option, illegal option combination, relation-ref log render gap, list in key per selected fork, generated collision, recursive template dependency, aggregate overflow/bound |
| parity | existing option and enum fixtures byte/term parity where the generalized mechanism claims equivalence; current cited unsupported constructs retain names until a fork explicitly replaces them |

### Remaining mechanism forks

| question | A | B | price difference |
|---|---|---|---|
| artifact vocabulary | templates return raw `Decls/Rules` | templates return typed artifact records lowered afterward | B adds an IR/lowering pass; A couples templates to current Prolog term shapes |
| canonical name readability | reversible length-prefixed encoding | readable stem plus stable digest | A names grow with nesting; B requires digest algorithm/version pin and collision check |
| generated artifact ordering | global canonical sort | dependency topological order with canonical tie-break | global sort can separate related declarations; topo order needs stored dependency edges |
| template rule policy | all selected artifacts automatic | declarations only | §2 costs apply |
| persisted-name evolution | schema version in every canonical name | explicit migration table only when representation changes | versioned names cause planned churn; unversioned names require representation compatibility checks |
