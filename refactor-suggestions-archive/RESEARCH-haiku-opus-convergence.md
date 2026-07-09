# Pushing Haiku to the Opus partition — deterministically

## The gap is synthesis, not detection

From the 12-run stats: Haiku and Opus **cut at the same seams** (the duplication clusters; C1/C6 were
12/12 across both tiers). They diverge on the **shape of the extracted type**:

| Motif | Opus picks | Haiku picks |
|-------|-----------|-------------|
| co-travel set | `struct NameResolver{…}` (noun) | `fn<F,T>(closures)` (verb) |
| isomorphic-fan | `enum`/`BuiltinGroup` table | `HashMap<String,…>` / macro |
| shared discriminator | `enum EdgeKind` + dispatch | leave as `&str` |

So the model gap is a gap in **abstraction synthesis** (turning a detected repetition into a named
type), not in **seam detection** (finding the repetition). That matters because the deterministic
detector *does the synthesis step* — `motif → shape` is a pure function of the geometry (`co-travel→struct`,
`fan→table`, `discriminator→enum+trait`). The tool can hand Haiku the answer Haiku can't synthesize.

## π is the determinization of "what shape" — it encodes Opus's taste

Why does Opus pick a noun and Haiku a verb? They read different axes of the same repetition:
- Opus reads the **data** shape — "these values travel together" → a product → a **struct/enum** (noun).
- Haiku reads the **control** shape — "this logic repeats" → a function → a **generic fn / map** (verb).

The π-projection over the code graph makes this a lookup, not a judgment. A co-travel cluster is a **⊗
(product) motif** by construction (it's a recurring field/arg multiset in `type_sig`/`type_edge`), and a
product's canonical home is a struct. A shared-discriminator cluster is a **⊕ (sum) motif**, whose home
is an enum + one dispatch. **The motif→shape table is literally "Opus's taste" compiled to a function.**
So the tool doesn't need Haiku to be smarter; it computes the partition Opus would have chosen.

## Three intervention points (determinism increasing, model-burden decreasing)

| Lever | What the tool provides | What the LLM still does | Determinism |
|-------|------------------------|-------------------------|-------------|
| **L1 rubric** | the π-lattice + motif→shape table, in the prompt | find seam, classify motif, apply mapped shape | soft (model still classifies) |
| **L2 tool-seam** | the detected cluster (sites) + the motif label | pick shape via the table, write the code | medium (synthesis given, application free) |
| **L3 tool-shape** | cluster + motif + **the chosen type + signature** | mechanically apply the struct/enum to K sites | hard (no design left → model gap vanishes) |

The thesis: **as you move L1→L3, Haiku's output converges to Opus's, because each step removes more of
the synthesis the gap lives in.** At L3 the LLM is an *applier*, and apply-not-design is where Haiku
already matches Opus. You don't lift Haiku; you relocate the hard part into the deterministic tool.

Bonus: L2/L3 also kill Haiku's two weaknesses observed in the stats — high variance and ~3× tool calls.
Handing it the seam removes the *search*, so it stops thrashing.

## The convergence experiment (measures whether the tool pushes Haiku → Opus)

Hold the seam set fixed (the detector's clusters). For each cluster, collect the shape each condition
produces, and score **shape-agreement** against two targets: Opus's shape and the tool's deterministic
shape (which should equal Opus's where the tool is right).

Conditions (Haiku only, since Opus is the reference):
- **H0 cold** — current prompt (baseline; expect low agreement).
- **H1 rubric (L1)** — prompt + π-lattice + motif→shape table.
- **H2 tool-seam (L2)** — given the cluster + motif label, asked for the shape.
- **H3 tool-shape (L3)** — given the type + signature, asked to apply it.

Metrics:
- `agree_opus(condition)` = % clusters where Haiku's shape == Opus's shape.
- `agree_tool(condition)` = % where Haiku's shape == the deterministic shape.
- `apply_correct(H3)` = % where the mechanical application compiles / is behavior-preserving.

Hypotheses:
- H0 ≈ baseline (the ~tier-split we already measured), H1 > H0, H2 ≫ H0, **H3 → ~1.0**.
- Where `agree_tool` ≫ `agree_opus` at L1/L2, the tool is *more* consistent than Opus — the tool, not
  Opus, becomes the reference, and Opus's role drops to calibrating the motif→shape table.

This is the same study design as the earlier convergence work, but the independent variable is **how
much of the synthesis is deterministically supplied** instead of **which model**.

## What "deterministically push it that way" means precisely

The tool doesn't change Haiku's reasoning; it **changes the task Haiku is given** so the part Haiku is
bad at is already done:
1. Detector finds the seam (graph query — no LLM).
2. π classifies the motif and looks up the shape (pure function — no LLM, this is the ex-Opus step).
3. LLM applies the named type to the sites (Haiku-sufficient).

The model gap closes not because Haiku got better but because steps 1–2 — detection and synthesis —
left the model entirely. The LLM is demoted to a mechanical applier, and at that altitude Haiku ≈ Opus.

## Caveat: keys-out for recall, keys-in for precision

The tool projects keys out to *find* the partition (recall). Restoring keys is where a wrong merge gets
caught (`Point{x,y}` vs `Size{w,h}` share a shape but must not merge). So L3 still needs a precision
gate: a key-aware / usage-aware check, or the human/agent-ratchet, before the apply lands. That gate is
deterministic-checkable for the structural cases (do the call sites actually pass the same *roles*?) and
only falls back to a model when semantics are genuinely ambiguous — the one place the model's judgment,
not its synthesis, is still load-bearing.

## One-line answer

Don't make Haiku reach the Opus *conclusion*; make the deterministic tool **produce** the conclusion
(motif→shape is Opus's taste as a pure function) and hand it to Haiku to apply. The convergence
experiment (H0→H3) measures exactly how much determinism it takes to collapse the tier gap, and the
expectation is that at L3 it collapses fully.
