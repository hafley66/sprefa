---
created: 2026-08-16
updated: 2026-08-17
type: bug
reporter: fable
status: open
priority: high
epic: bug-mining
labels:
- bugmine
- pkg:engine-rs
- size:med
lane: engine-source-bind
lane_seq: 10
collision: [source-bind-runtime, engine-hosts]
related: ['@soopy-extract-host-reads']
---

# Extract host reads worktree disk under a rev pin; digest is freshness not identity

_Source: v6/tsv2/goldens/scip_combo/2_extract_rev_skew.dl6 (pinned defect F3)_

## Description

## Comments

### 2026-08-16T05:31:17Z · @fable

Mechanism (PR #291): the extract host reads the path off the worktree disk regardless of rev pin, and digest is freshness, so two demands under two rev digests share one response identity. Pinning the file set does not pin the extraction. Design-flavored: response identity needs the content id, not the path.

### 2026-08-17T13:11:36Z · @stale-grader

Verdict: SUPERSEDED for the stated mechanism. Graded against origin/main 10166672f, not against the card.

The worktree-disk read is fixed on main. `SprefaExtractExecutor::read_blob` (v6/sprefa-engine-rs/src/hosts.rs:898-919) reads the Git blob named by the `digest` input through one `git cat-file --batch` per repository root, memoized in `batches: Mutex<BTreeMap<String, soopy::GitBatch>>` (hosts.rs:119); `run` dispatches on a non-empty `digest` for ANY host name and falls back to the worktree read only when no digest arrives (hosts.rs:968-990). Test `digest_carrying_demand_reads_the_blob_not_the_worktree` (tests/live_hosts.rs) passes on main. The golden's own header states it: v6/tsv2/goldens/scip_combo/2_extract_rev_skew.dl6:1-11.

The second half of the card, "two demands under two rev digests share one response identity", does not hold on main either. `generated_host_decls/7` emits `keyed(ResponseRef, [1, 2])` = (witness_digest, ordinal) (v6/prolog/1_host_expand.pl:551-557), and a freshness input extends the witness digest (`digest_expr(witness, ...)`, 1_host_expand.pl:486). Two revisions of one path are two witnesses and therefore two response identities.

The stranded commit is /private/tmp/sprefa-extract-rev-pin-identity, branch fix/extract-rev-pin-identity, commit 093cb59d1 (135 commits behind main, never PR'd). Its hosts.rs half is the same idea as main's in a weaker form: `GitBatch::open` per call rather than a memo, and gated on the literal host name `repo_extract` rather than on the presence of a digest. Its prolog half is NOT on main: a new `content_identity` input role (registry.pl `identity_input_role/1`, `1_host_expand.pl:validate_content_identity/2`) that is identity for digest purposes but exempt from the "identity input must appear in the template" check, plus flipping `host_input_contract(repo_extract, ...)` from `[identity, identity, freshness]` to `[identity, identity, content_identity]`.

Not landed, and this is why. Flipping that role moves `digest` from the salt list into `Inputs` at `partition_host_columns/4` (1_host_expand.pl:509-519), which changes the generated demand rel's arity, the generated response rel's arity, and what the identity digest is computed over. The card itself calls the change design-flavored, ARCH.pl `cold_author_defects` names D1 ("host_input_contract keyed on hardcoded host NAMES") as a registry DESIGN card, and CLAUDE.md's standing law is that language and type-plane design happens with Chris in the room. The commit also carried no fixture, conformance or sweep receipt for the flip.

Exact remainder if the design is ruled: (1) decide whether an out-of-band content address is a third input role or whether host input contracts stop being keyed on hardcoded names (ARCH D1); (2) apply the role change; (3) re-render 2_extract_rev_skew.dl6's header, which currently documents the freshness reading as load-bearing for its two-rule inversion (lines 20-33); (4) receipts = conformance, sweep both modes, scip-combo, hosts_wiring.

Leaving the card OPEN, retitled in substance: the extraction read is pinned, the ROLE question is what is left, and it is a design fork, not a bug. /private/tmp/sprefa-extract-rev-pin-identity is left on disk for Chris to prune; nothing in it is worth landing as-is.

