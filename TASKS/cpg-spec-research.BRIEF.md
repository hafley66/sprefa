# cpg-spec-research: Joern CPG spec inventory + generic CFG feed pricing

Repo: sprefa. Base sha: 89432169d5c3593e22a0c2113ddacedfd2fe14d5 (origin/main).
FIRST ACTION: `git merge --ff-only 89432169d5c3593e22a0c2113ddacedfd2fe14d5`. Failure = STOP AND REPORT.

READ-ONLY RESEARCH. You write exactly ONE file:
`plans/2026-08-16-cpg-spec-research.REPORT.md`. No code changes, no other files.
Anchor doc to read first: `plans/2026-08-16-joern-cpg-striking-distance.md`.
Issue: `issues/cpg-spec-research/item.md`.

## Four questions. Every claim carries a citation (URL + section, or file:line).

1. **Joern CPG spec vocabulary.** From the published Code Property Graph
   specification (cpg.joern.io / the joern GitHub spec), enumerate EVERY edge
   kind with its one-line semantic, and the node kinds relevant to statements
   and expressions (METHOD, CALL, IDENTIFIER, LITERAL, CONTROL_STRUCTURE,
   BLOCK, LOCAL, ...). Never from memory; quote the spec. Output: one table.

2. **tree-sitter-graph.** Can its per-language .tsg rule files express a
   kind_role mapping (CST kind -> branch/loop/jump/exit role)? What is its
   runtime (rust crate? standalone?), license, and maintenance state? Would we
   consume .tsg files or only borrow the idea? Output: a short candidate
   analysis, no one-line dismissal.

3. **CPG protobuf import.** Locate the CPG serialization schema (protobuf) in
   the Joern/codepropertygraph repo. Assess an importer shaped like our SCIP
   importer (`v6/sprefa-extract/src/scip_decode.rs` + vendored proto at
   `src/scip/scip_proto.rs`): what messages exist, is the schema versioned,
   what subset maps onto our families. Output: feasibility verdict + the
   message names that matter.

4. **kind_role census.** For the four grammars we ship (rust, go, kotlin, ts):
   enumerate the CST node kind names that play branch, loop, jump, and exit
   roles. Sources: the tree-sitter grammar node-types (the crates vendored in
   v6/sprefa-extract's Cargo tree) or the kinds visible in CstF output. Run
   `cargo tree -p sprefa-extract` in v6/sprefa-extract to find grammar crate
   versions; node-types.json inside each grammar crate is the roster. Output:
   one table per lang, kind name -> role, with the source cited.

## Report shape

Open with a TOC. Tables over prose. End with a "what this changes in the
anchor doc" section: any claim in plans/2026-08-16-joern-cpg-striking-distance.md
your findings contradict, listed bluntly.

Style: no em dashes; banned words provenance/substrate/load-bearing/regime;
"refusal" stays out of prose; construct naming uses rxjs/prolog/SQL vocabulary.

Commit the one report file on your branch. Never push. Never touch main.
