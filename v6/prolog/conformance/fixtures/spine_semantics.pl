% fixtures/spine_semantics.pl : conformance fixtures for the NEW fs/git/rev
% spine (plans/2026-07-27-fs-rev-spine.md), not a lab promotion -- these
% fixtures model the type spine directly. Machinery (crawling, extraction,
% watching) is solved in rust and enters the engine as binds; every "world"
% input below is a canned scheduled arrival (the bind-at-link law), never a
% real process spawn or filesystem walk.
%
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)
%
% STORAGE LAW (plan, "INTEGER KEYS EVERY TIME"): every GitRepo/GitRev/
% FileSpan identity column below is a plain Prolog integer (repo_id, rev_id,
% span_id). Path, digest, ref_text, and enum-tag columns are presentation-
% shaped and stay strings/atoms; they are never joined on as a spine
% identity in these fixtures.
%
% S2 (split file rels): worktree_file (mutable, keyed by path) and tree_file
% (immutable, keyed by (rev, path)) are separate rels throughout, per the
% ruling in the plan; diff-shaped rules join both, paying S2's cost at the
% rule level rather than collapsing them into one rel of mixed lifetime.
%
% S3 (dirty is derived, not an identity): dirty/1 is always a level rule
% comparing worktree_file against tree_file at the head rev. No fixture here
% carries a Dirty(Oid)-shaped rev value; group 2 is the ruled reading.
%
% Engine constraint honored throughout: no departed/gone body form exists
% yet (concurrently landing); every retraction here is graded only through
% deltas(...) on a level view, never a departed-body rule.
%
% Suffix/string-match note (group 1): "rust_file selecting by suffix match"
% is not expressible -- stdlib string fns beyond concat are out of scope
% per FIXTURES.md. Fixtures use a pre-classified kind column instead (the
% shape a real extraction bind would hand the engine), matching how
% check_eventing.pl treats `contains(text, "eprintln!")` as out of scope.
%
% Not exercised here (own territory, not attempted): the mixed-head
% git_repo pattern (world-fed AND rule-derived rows sharing one head,
% ARCH.pl callout) belongs to the ARCH.pl mixed-head arc; group 4 below keeps
% known_repo and repo_candidate as two separate rels instead of testing
% that construct. FileSpan byte-span columns (start/end) are not unpacked
% anywhere; xref carries span ids as opaque integers only.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ group 1: WORKTREE REPLACE ══════════════════════════════════════════════
% worktree_file keyed by path, digest column; an edit arrival replaces
% (-old/+new); an identical re-save is silent (equal-row no-op, r_equal_row_
% write). The level view (rust_file) is driven by a pre-classified kind
% column carried on the edit itself, and flips only when Kind flips.

fixture(worktree_edit_replaces_digest_and_flips_kind_view,
  prog([ kind(worktree_edit/3, log), keep(worktree_edit/3, all),
         keyed(worktree_file/3, [1]) ],
       [ (worktree_file(Path, Digest, Kind) <+ worktree_edit(Path, Digest, Kind)),
         (rust_file(Path) <- worktree_file(Path, _, rust)) ]),
  [],
  [ [ +worktree_edit("src/main.rs", "sha_a1", rust) ],
    [ +worktree_edit("src/main.rs", "sha_a2", rust) ],
    [ +worktree_edit("src/main.rs", "sha_a3", other) ] ],
  [ deltas(worktree_file/3,
      [ [ +worktree_file("src/main.rs", "sha_a1", rust) ],
        [ -worktree_file("src/main.rs", "sha_a1", rust), +worktree_file("src/main.rs", "sha_a2", rust) ],
        [ -worktree_file("src/main.rs", "sha_a2", rust), +worktree_file("src/main.rs", "sha_a3", other) ],
        [] ]),
    deltas(rust_file/1,
      [ [ +rust_file("src/main.rs") ], [], [ -rust_file("src/main.rs") ], [] ]),
    final(worktree_file/3, [ worktree_file("src/main.rs", "sha_a3", other) ]),
    final(rust_file/1, []) ]).

fixture(worktree_edit_identical_resave_is_silent,
  prog([ kind(worktree_edit/3, log), keep(worktree_edit/3, all),
         keyed(worktree_file/3, [1]) ],
       [ (worktree_file(Path, Digest, Kind) <+ worktree_edit(Path, Digest, Kind)) ]),
  [],
  [ [ +worktree_edit("README.md", "sha_r1", doc) ],
    [ +worktree_edit("README.md", "sha_r1", doc) ] ],
  [ deltas(worktree_file/3,
      [ [ +worktree_file("README.md", "sha_r1", doc) ], [] ]),
    final(worktree_file/3, [ worktree_file("README.md", "sha_r1", doc) ]) ]).

% ═══ group 2: DIRTY AS DERIVED (ruling S3) ══════════════════════════════════
% tree_file rows for the head rev + worktree rows; dirty(path) is a level
% join where digests differ. No special retraction machinery exists: dirty
% clears the instant a commit arrival makes tree_file agree with worktree_
% file at the new head, purely because the level view recomputes.

fixture(dirty_derives_from_digest_mismatch,
  prog([ kind(worktree_edit/2, log), keep(worktree_edit/2, all),
         keyed(worktree_file/2, [1]),
         keyed(tree_file/3, [1, 2]),
         keyed(head/2, [1]) ],
       [ (worktree_file(Path, Digest) <+ worktree_edit(Path, Digest)),
         (dirty(Path) <-
            worktree_file(Path, WorktreeDigest), head(_RepoId, RevId),
            tree_file(RevId, Path, TreeDigest), WorktreeDigest \== TreeDigest) ]),
  [ worktree_file("src/lib.rs", "sha_clean"), tree_file(100, "src/lib.rs", "sha_clean"), head(1, 100) ],
  [ [ +worktree_edit("src/lib.rs", "sha_dirty") ] ],
  [ deltas(dirty/1, [ [ +dirty("src/lib.rs") ], [] ]),
    final(dirty/1, [ dirty("src/lib.rs") ]) ]).

% THE S3 RECEIPT FIXTURE. Clean at start (worktree and tree_file digests
% agree at the current head). An edit makes dirty assert. A commit arrival
% (new rev rows for tree_file, modeled as an ordinary keyed insert since the
% key is a fresh (rev, path) pair) plus a head move (keyed replace on head)
% land in the SAME tick; the level join recomputes against the new head and
% dirty retracts, with no departed/gone form and no bespoke un-dirty rule.
fixture(dirty_retracts_on_matching_commit,
  prog([ kind(worktree_edit/2, log), keep(worktree_edit/2, all),
         keyed(worktree_file/2, [1]),
         kind(commit_arrival/3, log), keep(commit_arrival/3, all),
         keyed(tree_file/3, [1, 2]),
         kind(head_move/2, log), keep(head_move/2, all),
         keyed(head/2, [1]) ],
       [ (worktree_file(Path, Digest) <+ worktree_edit(Path, Digest)),
         (tree_file(RevId, Path, Digest) <+ commit_arrival(RevId, Path, Digest)),
         (head(RepoId, RevId) <+ head_move(RepoId, RevId)),
         (dirty(Path) <-
            worktree_file(Path, WorktreeDigest), head(RepoId, RevId),
            tree_file(RevId, Path, TreeDigest), WorktreeDigest \== TreeDigest) ]),
  [ worktree_file("src/lib.rs", "sha_v1"), tree_file(100, "src/lib.rs", "sha_v1"), head(1, 100) ],
  [ [ +worktree_edit("src/lib.rs", "sha_v2") ],
    [ +commit_arrival(101, "src/lib.rs", "sha_v2"), +head_move(1, 101) ] ],
  [ deltas(dirty/1, [ [ +dirty("src/lib.rs") ], [ -dirty("src/lib.rs") ], [] ]),
    final(dirty/1, []),
    final(worktree_file/2, [ worktree_file("src/lib.rs", "sha_v2") ]),
    final(tree_file/3, [ tree_file(100, "src/lib.rs", "sha_v1"), tree_file(101, "src/lib.rs", "sha_v2") ]),
    final(head/2, [ head(1, 101) ]) ]).

% ═══ group 3: HEAD MOVE AS KEY REPLACE ══════════════════════════════════════
% head(repo_id, rev_id) keyed by repo_id; a ref move arrival is an ordinary
% keyed write, -old/+new. A level view naming the current tree (the join of
% head and tree_file) flips its whole row set inside one tick's boundary;
% no intermediate mixed-rev state is ever observable.

fixture(head_move_replaces_key,
  prog([ kind(head_move/2, log), keep(head_move/2, all),
         keyed(head/2, [1]) ],
       [ (head(RepoId, RevId) <+ head_move(RepoId, RevId)) ]),
  [ head(1, 100) ],
  [ [ +head_move(1, 200) ], [ +head_move(1, 300) ] ],
  [ deltas(head/2,
      [ [ -head(1, 100), +head(1, 200) ],
        [ -head(1, 200), +head(1, 300) ],
        [] ]),
    final(head/2, [ head(1, 300) ]) ]).

fixture(head_move_flips_current_tree_in_one_tick,
  prog([ kind(head_move/2, log), keep(head_move/2, all),
         keyed(head/2, [1]),
         keyed(tree_file/3, [1, 2]) ],
       [ (head(RepoId, RevId) <+ head_move(RepoId, RevId)),
         (current_tree(Path, Digest) <- head(RepoId, RevId), tree_file(RevId, Path, Digest)) ]),
  [ head(1, 100),
    tree_file(100, "src/a.rs", "sha_a1"), tree_file(100, "src/b.rs", "sha_b1"),
    tree_file(200, "src/a.rs", "sha_a2"), tree_file(200, "src/b.rs", "sha_b2"), tree_file(200, "src/c.rs", "sha_c1") ],
  [ [ +head_move(1, 200) ] ],
  [ deltas(current_tree/2,
      [ [ -current_tree("src/a.rs", "sha_a1"), -current_tree("src/b.rs", "sha_b1"),
          +current_tree("src/a.rs", "sha_a2"), +current_tree("src/b.rs", "sha_b2"), +current_tree("src/c.rs", "sha_c1") ],
        [] ]),
    final(current_tree/2,
      [ current_tree("src/a.rs", "sha_a2"), current_tree("src/b.rs", "sha_b2"), current_tree("src/c.rs", "sha_c1") ]) ]).

% ═══ group 4: XREF POINTER ROWS ═════════════════════════════════════════════
% xref(from_span_id, to_repo_id, to_rev_id, to_path, to_span_id, kind) is a
% pure projection of an extraction-shaped pin arrival, keyed by from_span_id
% (re-extraction replaces the pointer). to_span_id is `none` here (no
% cross-repo span resolved yet); the plan's Option(FileSpan) column needs no
% new engine feature, it is the ordinary `none` value already used for an
% absent field. repo_candidate is a separate level-derived rel: discovery is
% a rule reading the same pin arrivals, gated by not(known_repo(...)).

fixture(pin_to_unknown_repo_derives_repo_candidate,
  prog([ kind(pin_extracted/5, log), keep(pin_extracted/5, all),
         kind(known_repo/1, set),
         keyed(xref/6, [1]) ],
       [ (xref(FromSpanId, ToRepoId, ToRevId, ToPath, none, Kind) <+
            pin_extracted(FromSpanId, ToRepoId, ToRevId, ToPath, Kind)),
         (repo_candidate(ToRepoId) <-
            pin_extracted(_, ToRepoId, _, _, _), not(known_repo(ToRepoId))) ]),
  [ known_repo(2) ],
  [ [ +pin_extracted(10, 2, 501, "vendor/lib.rs", import) ],
    [ +pin_extracted(11, 3, 602, "shared/proto.rs", import) ] ],
  [ deltas(repo_candidate/1, [ [], [ +repo_candidate(3) ], [] ]),
    deltas(xref/6,
      [ [ +xref(10, 2, 501, "vendor/lib.rs", none, import) ],
        [ +xref(11, 3, 602, "shared/proto.rs", none, import) ],
        [] ]),
    final(repo_candidate/1, [ repo_candidate(3) ]),
    final(xref/6,
      [ xref(10, 2, 501, "vendor/lib.rs", none, import), xref(11, 3, 602, "shared/proto.rs", none, import) ]) ]).

% xref carries the rev the DATA named (777, from the pin), never the live
% head. A head move for the SAME repo lands in the next tick and xref does
% not move: the derivation never joins head at all.
fixture(xref_rev_is_pin_data_not_live_head,
  prog([ kind(pin_extracted/5, log), keep(pin_extracted/5, all),
         kind(known_repo/1, set),
         keyed(xref/6, [1]),
         kind(head_move/2, log), keep(head_move/2, all),
         keyed(head/2, [1]) ],
       [ (xref(FromSpanId, ToRepoId, ToRevId, ToPath, none, Kind) <+
            pin_extracted(FromSpanId, ToRepoId, ToRevId, ToPath, Kind)),
         (head(RepoId, RevId) <+ head_move(RepoId, RevId)) ]),
  [ known_repo(2), head(2, 100) ],
  [ [ +pin_extracted(20, 2, 777, "docs/api.md", doc_link) ],
    [ +head_move(2, 200) ] ],
  [ deltas(xref/6, [ [ +xref(20, 2, 777, "docs/api.md", none, doc_link) ], [], [] ]),
    deltas(head/2, [ [], [ -head(2, 100), +head(2, 200) ], [] ]),
    final(xref/6, [ xref(20, 2, 777, "docs/api.md", none, doc_link) ]),
    final(head/2, [ head(2, 200) ]) ]).

% ═══ group 5: CHANGED SINCE AGENT TURN ══════════════════════════════════════
% agent_turn(turn_id, tick) is a Log rel fed by a canned turn_marker arrival
% plus now(Tick) in the edge rule (the tick machinery IS the change log,
% per the plan; no special agent_changed table). Worktree edits become a
% Log-rel change_event(path, digest, tick), stamped the same way. changed_
% since(turn_id, path) is a level join filtering on tick > the turn's tick.

fixture(changed_since_spans_two_turns,
  prog([ kind(worktree_edit/2, log), keep(worktree_edit/2, all),
         kind(change_event/3, log), keep(change_event/3, all),
         kind(turn_marker/1, log), keep(turn_marker/1, all),
         kind(agent_turn/2, log), keep(agent_turn/2, all) ],
       [ (change_event(Path, Digest, Tick) <+ worktree_edit(Path, Digest), now(Tick)),
         (agent_turn(TurnId, Tick) <+ turn_marker(TurnId), now(Tick)),
         (changed_since(TurnId, Path) <-
            agent_turn(TurnId, TurnTick), change_event(Path, _, EventTick), EventTick > TurnTick) ]),
  [],
  [ [ +turn_marker(turn_a) ],
    [ +worktree_edit("src/alpha.rs", "sha_1") ],
    [ +worktree_edit("src/beta.rs", "sha_2") ],
    [ +turn_marker(turn_b) ],
    [ +worktree_edit("src/gamma.rs", "sha_3") ] ],
  [ deltas(changed_since/2,
      [ [],
        [ +changed_since(turn_a, "src/alpha.rs") ],
        [ +changed_since(turn_a, "src/beta.rs") ],
        [],
        [ +changed_since(turn_a, "src/gamma.rs"), +changed_since(turn_b, "src/gamma.rs") ],
        [] ]),
    final(changed_since/2,
      [ changed_since(turn_a, "src/alpha.rs"), changed_since(turn_a, "src/beta.rs"),
        changed_since(turn_a, "src/gamma.rs"), changed_since(turn_b, "src/gamma.rs") ]) ]).

% A change_event that predates the turn (seeded at data-tick 0, before any
% turn marker ever arrives) never counts; the strict `>` is the whole rule.
fixture(changed_since_ignores_events_before_turn,
  prog([ kind(worktree_edit/2, log), keep(worktree_edit/2, all),
         kind(change_event/3, log), keep(change_event/3, all),
         kind(turn_marker/1, log), keep(turn_marker/1, all),
         kind(agent_turn/2, log), keep(agent_turn/2, all) ],
       [ (change_event(Path, Digest, Tick) <+ worktree_edit(Path, Digest), now(Tick)),
         (agent_turn(TurnId, Tick) <+ turn_marker(TurnId), now(Tick)),
         (changed_since(TurnId, Path) <-
            agent_turn(TurnId, TurnTick), change_event(Path, _, EventTick), EventTick > TurnTick) ]),
  [ change_event("src/old.rs", "sha_0", 0) ],
  [ [ +turn_marker(turn_only) ],
    [ +worktree_edit("src/new.rs", "sha_9") ] ],
  [ deltas(changed_since/2, [ [], [ +changed_since(turn_only, "src/new.rs") ], [] ]),
    final(changed_since/2, [ changed_since(turn_only, "src/new.rs") ]) ]).

% ═══ group 6: REV RESOLUTION AS DEMAND (the pin-skew shape) ═════════════════
% Restates v5's rev_cmp_want/rev_behind (examples/pin-skew.dl) in the v6
% shape: pins demand rev ancestry, a keyed Set rel dedups two pins wanting
% the same (repo, ref) to one demand row, and a canned fill arrival a tick
% later plays the `git rev-list`-backed bind. stale_pin is the level view
% over the filled behind/ahead counts.

fixture(two_pins_dedup_to_one_demand_row,
  prog([ kind(pin_want/3, log), keep(pin_want/3, all),
         kind(demand_rev/2, set), keyed(demand_rev/2, [1, 2]),
         kind(rev_fill/4, log), keep(rev_fill/4, all),
         keyed(rev_status/4, [1, 2]) ],
       [ (demand_rev(DepRepoId, RefText) <+ pin_want(_, DepRepoId, RefText)),
         (rev_status(DepRepoId, RefText, Behind, Ahead) <+
            rev_fill(DepRepoId, RefText, Behind, Ahead), latest(demand_rev(DepRepoId, RefText))),
         (stale_pin(DepRepoId, RefText) <- rev_status(DepRepoId, RefText, Behind, _), Behind > 0) ]),
  [],
  [ [ +pin_want(1, 9, "main"), +pin_want(5, 9, "main") ],
    [ +rev_fill(9, "main", 3, 0) ] ],
  [ deltas(demand_rev/2, [ [ +demand_rev(9, "main") ], [], [] ]),
    deltas(rev_status/4, [ [], [ +rev_status(9, "main", 3, 0) ], [] ]),
    deltas(stale_pin/2, [ [], [ +stale_pin(9, "main") ], [] ]),
    final(demand_rev/2, [ demand_rev(9, "main") ]),
    final(rev_status/4, [ rev_status(9, "main", 3, 0) ]),
    final(stale_pin/2, [ stale_pin(9, "main") ]) ]).

fixture(rev_fill_not_behind_keeps_stale_pin_empty,
  prog([ kind(pin_want/3, log), keep(pin_want/3, all),
         kind(demand_rev/2, set), keyed(demand_rev/2, [1, 2]),
         kind(rev_fill/4, log), keep(rev_fill/4, all),
         keyed(rev_status/4, [1, 2]) ],
       [ (demand_rev(DepRepoId, RefText) <+ pin_want(_, DepRepoId, RefText)),
         (rev_status(DepRepoId, RefText, Behind, Ahead) <+
            rev_fill(DepRepoId, RefText, Behind, Ahead), latest(demand_rev(DepRepoId, RefText))),
         (stale_pin(DepRepoId, RefText) <- rev_status(DepRepoId, RefText, Behind, _), Behind > 0) ]),
  [],
  [ [ +pin_want(4, 11, "release") ],
    [ +rev_fill(11, "release", 0, 2) ] ],
  [ final(demand_rev/2, [ demand_rev(11, "release") ]),
    final(rev_status/4, [ rev_status(11, "release", 0, 2) ]),
    final(stale_pin/2, []) ]).
