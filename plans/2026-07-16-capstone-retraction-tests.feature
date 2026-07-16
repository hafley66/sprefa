# Gherkin spec for plans/2026-07-16-capstone-retraction-test-plan.md
# Tier tags mirror the plan. One scenario = one test to implement.

Feature: T1 reconcile — row diffing between memoized and fresh derivation
  The diff engine every RowDelta flows through. Full-tuple identity.

  Background:
    Given a memoized row set "prev"
    And a freshly derived row set "next"

  Scenario: identical sets produce an empty delta
    Given "next" equals "prev"
    When reconcile runs
    Then the delta has no retracted rows
    And the delta has no inserted rows

  Scenario: never-derived family inserts everything
    Given "prev" is empty
    And "next" has 3 rows
    When reconcile runs
    Then the delta inserts exactly those 3 rows
    And retracts nothing

  Scenario: fully-gone family retracts everything
    Given "prev" has 3 rows
    And "next" is empty
    When reconcile runs
    Then the delta retracts exactly those 3 rows
    And inserts nothing

  Scenario: disjoint sets swap completely
    Given "prev" and "next" share no rows
    When reconcile runs
    Then every "prev" row is retracted
    And every "next" row is inserted

  Scenario: overlapping sets diff to the exact set difference
    Given "prev" is {A, B, C}
    And "next" is {B, C, D}
    When reconcile runs
    Then the delta retracts exactly {A}
    And inserts exactly {D}

  Scenario: duplicate rows in the fresh derivation
    Given "next" contains the same row twice
    When reconcile runs
    Then the observed multiplicity behavior is asserted as-is and documented
    # pin whatever row_key actually does; do not change the implementation

  Scenario: type affinity does not merge tuples
    Given "prev" holds a row with integer 1 in a column
    And "next" holds the same row with text "1" in that column
    When reconcile runs
    Then the two rows are treated as different tuples
    And the delta retracts one and inserts the other

  Scenario: NULL identity is pinned
    Given "prev" and "next" both hold one row containing a NULL column
    When reconcile runs
    Then the observed NULL-equality behavior is asserted as-is and documented

Feature: T1 retract_rows — chunked full-tuple DELETE under the param budget
  P0 (4eb5694a) fixed param-budget overflow; these pin the boundaries.
  The budget constant is read from the code, never hardcoded in tests.

  Scenario Outline: chunk boundaries
    Given a relation seeded with <seeded> rows of <cols> columns
    When retract_rows is called with <retract> of those rows
    Then the statement executes in <chunks> chunk(s)
    And exactly <retract> rows are gone afterward

    Examples:
      | seeded            | cols | retract          | chunks |
      | 10                | 3    | 0                | 0      |
      | 10                | 3    | 1                | 1      |
      | budget            | 3    | budget           | 1      |
      | budget + 1        | 3    | budget + 1       | 2      |
      | 2 * budget - 1    | 8    | 2 * budget - 1   | ceil-by-params |

  Scenario: retracting a row stored twice
    Given a relation holding the same tuple twice
    When retract_rows is called with that tuple once
    Then the observed row count afterward is asserted as-is and documented

  Scenario: retracting an absent row
    Given a relation that does not contain tuple X
    When retract_rows is called with tuple X
    Then no error is raised
    And the affected-row count is 0

Feature: T2 render flip — cold reload, warm delta, one transaction
  flip_call_rels_via_router is the sole writer of every public call rel.

  Scenario: cold family on a fresh database
    Given an engine with no memo for any call family
    When the flip runs
    Then every family renders via authoritative reload
    And each public rel equals its fresh derivation

  Scenario: cold reload removes rows a prior process left behind
    Given a public call rel pre-seeded on disk with a stale row
    And an engine whose in-process memo is empty
    When the flip runs
    Then the stale row is gone
    # INSERT OR IGNORE could never remove it; this is the cold guard's reason to exist

  Scenario: warm family with an empty delta issues no writes
    Given a warm engine whose inputs changed but rederive identical rows
    When the flip runs
    Then the family is reported as rerun
    And zero retract or insert calls hit its public rel

  Scenario Outline: warm family applies its delta exactly
    Given a warm engine and an input change producing a <shape> delta
    When the flip runs
    Then the public rel equals the router memo row-for-row

    Examples:
      | shape         |
      | insert-only   |
      | retract-only  |
      | mixed         |

  Scenario: a mid-render failure rolls back every rel
    Given three families will rerun
    And the second family's insert is forced to fail
    When the flip runs and errors
    Then all public call rels equal their pre-flip state
    And a subsequent flip succeeds and converges
    # memo must not be poisoned by the failed render

  Scenario: flip inside a caller-owned transaction
    Given the connection is already inside a transaction
    When the flip runs
    Then it issues no nested BEGIN
    And the outer transaction still owns the commit

  Scenario: react_deltas reports every rerun family
    Given a changed-set touching only _call_resolution
    When react_deltas runs
    Then the returned families are exactly call_edge and call_edge_rev in registry order
    And families with empty deltas are still present in the return
    And families whose inputs did not change are absent

Feature: T3 edit scripts — incremental engine equals fresh engine
  Each scenario ticks the engine per step, then compares all 7 public call
  rels against a fresh engine extracting the final tree. Deletion-heavy.

  Scenario: add then delete a file
    Given a tree where a file was added and ticked
    When the file is deleted and ticked again
    Then all 7 rels match a fresh engine on the final tree
    And no orphan call_site or call_edge rows remain

  Scenario: delete only the callee
    Given caller f calls g in another file
    When g's file is deleted and ticked
    Then the f→g call_edge is retracted
    And f's call_site row remains

  Scenario: rename a function in one tick
    Given fn alpha exists and is called
    When alpha is renamed to beta in a single edit and ticked
    Then no duplicate syms exist
    And the call_edge targets beta

  Scenario: whitespace-only edit
    Given a tick over an edit that changes no extracted facts
    When the flip runs
    Then families rerun with empty deltas
    And zero rows move in any public rel

  Scenario: retired rev is swept from all six owned tables
    Given rows for rev R exist in every owned _call_* table
    When R leaves the live rev scope and the sweep runs
    Then all six owned tables hold zero rows for R
    And deletion order respects the FK dependency chain
    And the flip fires only because rows moved

  Scenario: sweeping twice is idempotent
    Given rev R was already swept
    When the sweep runs again
    Then zero rows move
    And no flip fires

  Scenario: gone rev interleaved with a live rev sharing files
    Given revs A and B share file paths and A is retired
    When the sweep runs
    Then every rev-B row survives untouched

  Scenario Outline: degenerate trees
    Given a working tree that is <tree>
    When a full refresh and flip run
    Then all 7 public rels are empty
    And no error is raised

    Examples:
      | tree                    |
      | empty                   |
      | rust files with no calls |

  Scenario: add and delete the same file in one tick
    Given a file created and removed before the next tick
    When the tick runs
    Then all 7 rels match a fresh engine that never saw the file

Feature: T4 properties — randomized edit scripts against the oracle
  proptest; seeded and shrinkable; ~200 cases in CI, ~10k nightly.

  Scenario: equivalence holds at every step
    Given a random edit script of 5 to 15 steps over a synthetic tree
    When the engine ticks each step
    Then after EVERY step all 7 rels match a fresh engine on that step's tree

  Scenario: the memo never diverges from the rendered rel
    Given a random edit script
    When any step's delta is applied
    Then rows(memo) equals the public rel row-for-row after that step

  Scenario: empty changed-set is a fixpoint
    Given any engine state produced by a random script
    When the flip runs with an empty changed-set
    Then no family reruns
    And no rows move

Feature: T5 lifecycle — restarts, WAL readers, filesystem edges

  Scenario: memo loss across restart is safe
    Given an engine that committed and exited
    When a new process opens the same database and ticks a no-change set
    Then the cold reload reproduces byte-identical public rels

  Scenario: crash between input write and flip converges on restart
    Given the process is killed after owned _call_* writes but before the flip
    When a new process runs the next tick
    Then all public rels match a fresh engine
    # no permanently stale public rel

  Scenario: a WAL reader never observes a half-rendered rel
    Given a second connection reading during the flip transaction
    When it queries a public call rel repeatedly
    Then every read sees either the full pre-flip or full post-flip row set

  Scenario: same-timestamp equal-length edit is a documented gap
    Given the enumerate_with_hash mtime+size fast path
    Then no test is written against it until the fast path is fixed
    # ledger bug; a test here would only flake

Feature: T6 performance — counters, not clocks

  Scenario: writes scale with the delta, not the repo
    Given an N-file repo and a one-file edit
    When the tick runs
    Then the write-call count is proportional to the delta
    And independent of N

  Scenario: warm no-change tick writes nothing
    Given a tick whose changed-set intersects no family inputs
    Then zero rel writes occur

  Scenario: the per-row-write tripwire stays armed
    Given the tick counter rail
    When the full flip path runs on a real repo
    Then no per-row write loop fires the counter
    And slow-rule and tick-over-budget rails stay green
