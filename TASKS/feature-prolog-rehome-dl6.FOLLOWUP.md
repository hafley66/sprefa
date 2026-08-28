# feature-prolog-rehome-dl6: follow-up pass (coordinator answers, in order)

Your STOP hail (root identity) and your D1-green hail were received. Four
coordinator hails were held by the supervisor and never reached you. They are
reproduced here verbatim, oldest first. Apply all of them. Your worktree is
unchanged; nothing is committed yet.

## 1. Root identity (m-7167c841)

Verified both blockers (tests/15_source_mutation_hosts.rs:81-95 mints the id in
Rust; no host emits it). Decision: the EXECUTOR fills the root, the program
never spells it. Ownership extended to v6/sprefa-engine-rs/src/hosts.rs:520-600
and tests/15_source_mutation_hosts.rs (additive test only). Shape: in
source_stage_response parse request as serde_json::Value first; if the object
has no "root" key, open the root (SourceRoot::discover_git(target_root), fall
back to open_directory on error), build SourceRootId exactly as tests/15:81-95
does, insert it as "root", then from_value::<StageRequest>. A request that
carries "root" keeps today's strict path. Root kind for rehome.dl6: GitWorktree.
expected per Move/Replace: {"GitBlob": <files digest>} built with json_object;
Create carries no expected. Add one test in tests/15: stage with root omitted on
a git temp repo, assert outcome=staged and preview count.

You already built this shape in hosts.rs. Keep it.

## 2. .plt and test files (m-7bde7092)

v6/sprefa-extract/src/lang/prolog/_0_source.rs:761 matches() lacks .plt; add
path.ends_with(".plt") so v6/prolog/compile/test/2_subscribe.plt is a files row
like the 20 *.test.pl files. Test files (*.test.pl, *.plt) are NOT moved: they
stay put and keep resolving through the shims; assert in the dry run that zero
Move actions target a path under compile/test/ or ending .plt, and that every
relative use_module edge from a test file still resolves to a shim or unmoved
file (a reaches row per test file, count reported).

## 3. Diff review defects (m-dde94339, item 2 replaced by section 4)

(1) position(1..40) caps shim_byte: every shim_text is over 40 chars, so shims
get truncated bytes. Delete shim_byte/shim_byte_group/char_code/position
entirely; in the hosts.rs fill pass (same seam as the root fill) accept a Create
carrying "text": <string> and rewrite it to "bytes" as UTF-8 before from_value;
the program emits text. Add that to the tests/15 omitted-root test (a Create
with text, assert preview bytes).
(3) stem/2 uses substr(B,3): 10_expr.pl becomes _expr; strip up to and
including the first '_' via instr.
(4) tests/15 covers Create only; add one Move with source path + expected
{GitBlob: digest of a committed file} through the omitted-root path so
fill_git_action_sources' revision object is proven to pass plan_mutations.
Also assert per-folder distinct-depth count < 60 (int_text ceiling) as a ?
query and hail the counts.

## 4. Path ordering is deleted (m-ba8a04a6), and it is why your run hangs

Your `dl6 run` (pid 53474) sat at 100% CPU for 5 minutes in SQLite btree scans:
differ_at/earlier_diff are a position x position x char_at x char_at cross
product, 187 x 187 x 40 x 40 rows per fold. The coordinator killed it.

Delete less/less_at/differ_at/earlier_diff/char_at/before/comes_before and any
path tie-break. Ordinal inside a folder = dense rank of reach_count only (count
of DISTINCT reach_count values strictly below this file's, within the folder);
files with equal depth share the number. No text ordering exists in the
language and none will be added.

## 10-second law

Any single `dl6 run` over 10 s is a defect you investigate (sample the pid,
read the rule shape), never a wait. Do not `sleep N; ps -p` in a loop again.
Use the release binary (cargo build --release --bin dl6) for the real run.

## Then

Continue D2/D3 as briefed: COMPILE-TRACE, dry run staged with clean git status,
move/create counts, golden, commit, PR. Hail sprefa-coordinator at each
milestone with numbers.
