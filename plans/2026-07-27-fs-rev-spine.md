# fs / git / rev spine for v6 (boiling pot; user NOT sold on the v5 shape)

User inputs 2026-07-27: "we typed WORK pretty much the entire time and some
were not work, were just current tree"; "HEAD is a default we should never
see"; "we want to know when file changed last ai turn"; "the way we watch
files is variable"; scale target = 500 repos of hand-programmable parse
rules, rust-fast eventually.

## v5 inventory (what is being replaced; receipts in src/engine/{scan,revid,repo}.rs)

- `scan(repo, rev, glob, path, rev_out)`, arity defaults; rev in WORK|HEAD|sha.
- WORK = alias resolved per tick to HEAD-oid + `+` dirty suffix (revid.rs
  INV-1: no alias ever stored). One walk per repo x rev per tick over the
  union of globs; mtime+size fast path with the racy-write guard; git revs
  via ls-tree blob oids, no content fetch.
- `file(repo, rev, path, content)` eager content in-row (the 39x db/corpus
  ratio defect); `rev(id, repo, oid, ts)`.
- Watch = notify crate in the daemon; agent hook writes agent_changed.

## The candidate INVERSION (the different idea asked for)

v5 made rev a COORDINATE every scan names, because v5 had no native time.
v6 has time (ticks, Log rels, stamps, pre). So flip it:

1. THE LIVE TREE IS THE DEFAULT WORLD. `rel file(path: Key(Path), digest:
   Digest) from world;` No rev column. Lint rails (the 90%) join this and
   never name a coordinate. Nobody types WORK because current is not a
   coordinate, it is the world.
2. REVS ARE DATA, NOT SYNTAX. `head(repo: Key(Repo), rev: Rev) from world`
   (a ref move is a key replace, -old/+new, IVM does the rest);
   `commit_log(repo, rev, parent, ts) from world` as a Log rel with a keep
   bound. HEAD stops being a spelling; "committed tree" is a join.
3. PINNED TREES ARE LAZY EFFECTS. `tree_file(rev: Key(Rev,1), path:
   Key(Path,2), digest: Digest) -> ...` demanded per rev; only revs a
   program actually queries (ratchet baselines, diffs) materialize. Rev
   values flow in from data (ratchet rows, commit_log, config), never from
   a rev literal in a scan call.
4. "CHANGED SINCE LAST AI TURN" IS A TIME QUERY, NOT A GIT QUERY. The
   engine already stamps arrivals; the file rel's delta trail IS the change
   log. `agent_turn(id, tick)` as a Log rel (hook-fed); changed-since =
   join of file deltas after the turn's tick. v5 needed agent_changed as a
   special table; v6 gets it from the tick machinery.
5. WATCHING IS A BIND, AND VARIABLE MEANS PER-DEPLOYMENT. FSEvents/notify,
   poll on a clock bucket, agent-hook push, git-status sweep, LSP didSave,
   gh webhook: all fill the SAME file/tree rels. Program text never names
   the mechanism (link-time protocol law). Initial walk vs incremental
   events = backlog replay vs arrivals, the exact any-atom/marker semantics
   the conformance fixtures pin.
6. CONTENT IS DEMAND-ONLY. `content(digest: Key(Digest)) -> Str` lazy;
   rows carry digests; blobs cross the coastline only under demand. Kills
   the v5 eager-content defect at the type level. Extraction rels key on
   (digest, pattern) per the transform law; digest salt = re-extract on
   change (two-salt law, already ruled).

## The named type spine (USER-SET 2026-07-27): File / FileSpan / GitRepo / GitRev

The type system the user imagined from the start; the OG archive carries the
receipts (sprefa-archive-20260428: `refs` = every extracted string occurrence
as (file, span, string); README:21 auto-checkout of `repo: X, tag: v2.1.0`
seen in config files). Types are rels, so each is a rel with a struct
reading; 1-1 type->table lowering (surface-boil noted-for-later) gives each
a table, surrogate ids, interning.

    struct GitRepo { origin: Url }            // identity = origin, NOT a local path;
                                              // a repo can exist undiscovered/uncloned
    struct GitRev  { repo: GitRepo, oid: Oid }
    struct File    { repo: GitRepo, path: Path, digest: Digest }
    struct FileSpan{ file: File, start: Int, end: Int }   // BYTE span (OG refs
                                              // shape); line/col = derived view

    rel repo_candidate(origin: Url) from world;           // the known-possible set
    rel git_repo(repo: Key(GitRepo));                     // the DISCOVERED set
    rel repo_ref(repo: Key(GitRepo, 1), name: Key(RefName, 2), rev: GitRev)
        from world;                                       // heads PLURAL: refs
    rel checkout(repo: GitRepo, rev: GitRev) -> Path;     // clone/worktree = effect
    rel tree_file(rev: Key(GitRev, 1), path: Key(Path, 2), digest: Digest) -> ();
    rel content(digest: Key(Digest)) -> Str;              // lazy blobs

- GitRepo is its own rel, and DISCOVERY IS A RULE: `git_repo` rows come from
  the candidate crawl AND from extraction (a config row naming a repo/tag
  derives a git_repo row + a demand on checkout). That is the OG README:21
  loop, expressible natively because mixed heads are sound under count-IVM
  (ARCH callout): world-fed and rule-derived rows may share the git_repo
  head, which v5's one-rel-one-kind law forbade. The corpus GROWS by
  fixpoint: extract -> discover -> checkout -> extract.
- FileSpan is the one location value: extraction hits, diag rows, waivers
  all carry it; deriving line/col is a view over content line starts. The
  v5 shape (five scattered line/col columns per rel) dies.
- File carries repo + digest; the live-tree default world and the pinned
  tree_file both project into File, which is what keeps diff-shaped
  programs one-typed (S2's cost paid at the type level, not by every rule).

## 500-repo consequences

- repo is a rel (world-fed from config/discovery), fan-out is a join;
  per-repo binds must be GROUPABLE (one bind covering a repo set, not 500
  hand-written binds) -- bind vocabulary needs a selector column.
- Most repos are cold: watch mechanism per repo group (hot = fs watcher,
  warm = clock-bucket poll, cold = on-demand walk under a tail ask). The
  demand-row machinery is the scheduler; no new construct.
- Digest dedup across repos/forks matters at this scale (same blob in N
  checkouts = one content row); content-addressing is the RAM/disk story.
- Entrypoints: per-repo facts (`entrypoint(repo, path, kind)`), joined by
  the parse rules; hand-programmable = the rules are ordinary v6 programs
  per repo group, not engine config.

## Cross-repo pointering (USER 2026-07-27: the actual mission shape)

Microservices / cross-repo maintenance: pointers for LSP info and manually
marked indicators, within one repo or across repos; a cross-repo reference
from outside a git root points INWARD at a potentially different rev, and
the rev is FOUND FROM DATA (openapi version files, manifest pins). v5
already does this; the receipts:

- examples/pin-skew.dl: go.mod manifests -> `pin(consumer, dep, ref)`
  edges; `rev_cmp_want(repo, ref, "HEAD")` is a DEMAND rel the builtin
  `rev_behind` fills next tick (one `git rev-list` per wanted pair, the
  data-driven latency contract). stale vs diverged pins from ancestry
  counts. Pseudo-version and shallow-clone limits handled loudly.
- examples/flow-services.dl: the wire hop. Two services never share a call
  edge; the connection exists only in the CONTRACT (openapi operationId,
  jsonp-extracted). `scan("*")` fans the join across every config repo:
  client arg -> endpoint param ACROSS checkouts.
- examples/openapi-lsp.dl + rtkq-op-recovery.dl: spec ops joined to client
  stubs and server handlers, LSP-shaped output.
- `anchor name = fs:body.` decls (README:281): the manually-marked
  indicator primitive, v1 scope = default scan root only.

v6 restatement (all existing constructs, no new ones):

    rel pin(consumer: GitRepo, dep: GitRepo, ref_text: Str);   // extraction-derived
    rel resolve_rev(repo: GitRepo, ref_text: Key(Str, 2)) -> GitRev;  // effect
    rel rev_behind(repo: GitRepo, rev: GitRev, base: GitRev) -> (behind: Int, ahead: Int);
    rel xref(from: FileSpan, to_repo: GitRepo, to_rev: GitRev, to_path: Path,
             to_span: Option(FileSpan), kind: XrefKind);       // THE pointer rel

- v5's rev_cmp_want/rev_behind "fills next tick" IS the v6 effect model
  verbatim: demand rows = magic rows, latency = the effect contract. The
  convention-read pattern stops being a convention and becomes the typed
  default.
- xref is the LSP currency: within-repo rows have to_repo = own and
  to_rev = live; cross-repo rows carry the rev the DATA names (pin,
  openapi version, image tag). Marked indicators head xref via extraction
  of marker comments or anchor-style facts.
- The discovery loop composes: a pin naming an unknown repo derives a
  repo_candidate row; checkout demand follows; the corpus grows to cover
  exactly what is pointed at, at the revs pointed at.

## The mastered sqlite-DD layer (USER 2026-07-27: "do not ignore or forget")

The spine is where prior iterations broke most, and the whole point of the
rxjs + sqlite labs was lowering to a variant of sqlite differential
dataflow. That mastery EXISTS and the spine lowering must target it, not
rediscover it:

- count-IVM in the rust store beat DRed 4-5x (ARCH algorithm row); the js
  engine v1 (v6/sprefa-store) carries the same count-IVM semantics 84/84;
  mixed heads sound under per-row support is a consequence.
- Measured tier law: swipl tabling 1156ms vs sqlite-count 50ms vs dd 26ms
  at 160k nodes (ARCH callout). Prolog checks; sqlite executes.
- The graph-on-disk research arc (skills, all verified empirically):
  i:graph-libs sqlite-native (+opus-redo +red-team: recursive CTEs cover
  4-6 of 7 graph ops on real data; the 4,000x reverse-traversal index
  cliff; depth-cap costs), relational-graph-patterns 01-07 (CTE spec
  limits, closure.c, SQL/PGQ, semi-naive/magic-sets prior art, where
  datalog-as-SQL stops scaling), the rejected libraries with receipts
  (GraphBLAS depth scaling, LadybugDB buffer-pool claims, ultragraph
  freeze peak, petgraph visit-trait port at 61 lines).
- v5 storage lessons feeding the 1-1 type->table lowering: dense
  dictionary ids, WITHOUT ROWID junctions, index-audit dc9b67b1
  (planner-honest demand filters, 771 -> 262 idx_), the lazy-rel-tier
  amplification autopsy (indexes = 57% of file bytes; VIEW-only tier).
- Only deltas cross the coastline; tick = one transaction; boundary
  diffing (R7) IS the dd delta stream restated. The conformance engine's
  multiset stamps are the reference semantics for what sqlite count-IVM
  must reproduce at scale.

Spine consequence: FileSpan/File/GitRev tables are the dense-id storage the
on-disk algorithms run over; enumeration/extraction write deltas, never
rebuild; closure/scc over spans and repo graphs run IN sqlite per the
researched patterns.

STORAGE LAW (user-set 2026-07-27): INTEGER KEYS EVERY TIME. No string
hashes and no strings as keys or FKs anywhere in the big graph storage.
Every spine type row (GitRepo, GitRev, File, FileSpan, symbols) gets a
dense integer surrogate id; every FK and every join column is that int.
Strings and content hashes exist exactly once, in the interning tables
(term -> id), read at the presentation edge only. An oid is interned like
any other string; a rev FK is an int. This is the v5 storage-diet
dictionary-id direction promoted from optimization to law.

## Open questions (numbered, for ruling when boiled)

S1. `from world` syntax: leading candidate (T1 keep in AGGREGATE), not
    banked. Mechanics: declares "no rules head this rel; a bind fills it";
    canned rows in tests are program-text identical. Needs the ruling.
S2. One file rel vs split worktree/tree_file. The inversion says split
    (mutable worktree keyed by path; immutable tree_file keyed by
    (rev,path) lazy). Cost: diff-shaped programs join two rels.
S3. Dirty state: is a dirty worktree a Rev value (v5: oid+`+`) or is
    dirtiness = worktree_file differing from tree_file at head? The
    inversion favors the latter (dirtiness is a derived rel, not an
    identity).
S4. Glob residency: per-rule in program text (v5, engine unions per walk)
    vs demand columns on the enumeration bind. 500-repo union walks argue
    for demand columns.
S5. Rev retention: keep bound on commit_log; gc of unreferenced tree_file
    revs (retention clause ranges over Log rels only today; keyed lazy rels
    need an eviction story -- ties to v5 plans/2026-07-19-lazy-rel-tier.md).
S6. Git access bind: build-vs-buy analysis REQUIRED before any bespoke line
    (git CLI subprocess (v5) vs gitoxide vs libgit2). Not started.
S7. Change attribution columns: does the file rel carry who/why (bind fills
    a source column) or is attribution a separate Log rel joined by tick?
S8. Prolog sqlite bindings (user note): ARCH law says prolog never runs
    fixpoints at scale (measured 1156ms vs 50ms sqlite at 160k nodes), so
    the conformance engine stays RAM-small by design. IF prolog ever needs
    direct db reads (checker reading corpus facts), candidates to research
    first: SWI prosqlite pack, SWI ODBC, FFI to sqlite3. Research task, not
    a build task.

## Lab wave (dispatchable once S2/S3 leanings exist)

- fs_spine: enumeration/watch/replace as conformance fixtures (canned
  arrivals; walk-then-watch = backlog replay; digest replace on edit).
- rev_spine: head moves as key replace, commit_log retention, dirty-as-
  derived (S3 candidate B) vs Dirty(Oid) (candidate A), graded both.
- content_laziness: demand-driven content, digest salts, eviction hole
  (S5) stated.
- turn_attribution: agent_turn Log rel + changed-since-turn join (the
  "last ai turn" ask end to end).
