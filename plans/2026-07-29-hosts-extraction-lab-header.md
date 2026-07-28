# HOSTS + EXTRACTION LAB (planner contract; user go 2026-07-29 "striking position")

Parity target: stopping-point programs 1 (ghcacher), 3 (--changed),
4 (sprefa-extract), 7 (rtkq) all block on this cluster. Design records
that bind this lab: spine_residency (extraction is stdlib rels + binds +
salts, never kernel), salt_minting = content_addressed, vocabulary law
(rx/prolog/SQL words), every snippet carries its rx lowering,
edb_definition (never-headed rel = pure subject), the extraction
ambiguities A12/A1/A4/A14 (plans/2026-07-27-extraction-spellings.md).

Lab home: v6/prolog/labs/hosts_extraction/ + ONE verdict doc
plans/2026-07-29-hosts-extraction-verdict.md. TOUCH NOTHING ELSE (a
concurrent arc owns compile/*; the lab is standalone .pl + checks, the
match-frontier/types-lab pattern). Labs die on landing.

## Questions to grade (each = executable checks, PASS-only stdout)

Q1 sh HOST DECL term form: spelling for
    sh fetch(ep: text, prev: text, status: int, tag: text, body: text)
      = `template with {ep} and $prev`.
    including: input/output column split (v5 inferred inputs by scanning
    the template; grade explicit-vs-inferred both ways), the `?` probe
    (demand row, content-addressed identity, salt columns), refusal
    shapes for template/column mismatch. Ghcacher's fetch is the worked
    example; the graded artifact is the fixture-shaped term.
Q2 BIND DECL: the clock bind activates by rel-name match with zero decl
    (the magic-rel hazard, filed). Grade `bind clock(secs: int,
    bucket: int).` (or better) as a decl that names world-fed rels;
    consequence for edb_definition (a bind-decl'd rel is EDB by
    declaration, not just absence).
Q3 QUERY LINE `? rel(args).` term form (the read surface ghcacher.dl
    ends with).
Q4 JSON: grade v5 gh-cache's jsonp field pull and json array-explode
    with correlated nested fields against landed decode/json_each.
    Expressible = show the fixture; residue = name it.
Q5 EXTRACTION SHAPE, the fork, graded BOTH WAYS with the same worked
    examples (a callgraph sg pattern + a span_line-class scan):
    (a) HOST shape: sg/ast as stdlib host rels at the world boundary,
        rows land as EDB arrivals, content-addressed on
        (file_digest, query_digest);
    (b) TERM-EXTRACT shape: decode-class op over a bound content
        string (the json precedent), rows minted in-rule.
    Criteria: incremental behavior on file change (delta size), salt
    sharing across rules, whether the op can feed edge rules, rx
    lowering honesty of each.
Q6 NATIVE TREE-SITTER QUERY TERM: a prolog term mirroring the ts query
    S-expression grammar, compiled to the query string. Grade fidelity
    feature by feature: node types, field names, captures (@name),
    anonymous nodes, predicates (#eq?, #match?), quantifiers (?, *, +),
    alternations, wildcards. Each unmappable feature = a named slot,
    never a silent drop. Same for one ast-grep pattern example: does
    the sg pattern surface reduce to the same term shape or need its
    own.
Q7 The four standing ambiguities A12 (from-world = nullary `->`?),
    A1 (glob residency), A4 (fence escape), A14 (comment_span bind):
    each either resolves as a consequence of Q1-Q6 (say how) or stays
    open with the dependency named.

## Grades

- Lab suite: swipl -q -l labs/hosts_extraction/lab.pl -g go -g halt,
  exit 0, PASS-only stdout, run twice.
- The ghcacher worked example: full program term including the Q1 sh
  form, checked by the lab's own harness (the program compiles in the
  lab's model; wiring into compile.pl is the FOLLOW-UP arc, not this
  lab).
- Every proposed spelling carries its rx lowering in the verdict (the
  law); a spelling with no honest lowering is refused in the verdict.
- No-drift: conformance go.pl and roundtrip.sh untouched and green.

## Deliverable

Verdict doc: per-question verdict tables with criteria visible,
priced spellings (no fiat), named slots for every ambiguity, and the
distilled fixture/5 candidates for the follow-up wiring arc.
