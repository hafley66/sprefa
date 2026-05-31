# Conversation concept graph (live)

A parallel semantic graph of this session. Open in **Markdown Preview** (`Cmd+Shift+V`)
to see it drawn. It grows each turn: the diff is logged at the bottom in a string
matchable form, then folded into the graph.

We do not solve layout. We **declare** nodes/edges; Mermaid lays them out. That is the
whole trick that makes "graph drawing" tractable here.

```mermaid
flowchart TD
  subgraph THEORY
    order[order theory / posets] --> lat[lattice: join / meet]
    lat --> fix[fixpoint - Knaster-Tarski]
    fix --> semi[semi-naive eval]
    tarj[Tarjan SCC] --> cond[condensation to DAG]
    ring[semiring: reach / shortest / count] --> mind[min-distance]
    strat[stratified negation]
    dd[DBSP / differential]
    pad[parsing-as-deduction = DCG dual]
  end

  subgraph ENGINE_BUILT
    clo[closure rule + reaches view]
    rf[reaches_from / reached_by]
    st[stratifier]
    ai[auto-index]
    rcg[resolved call graph]
    endl[ast end-line binding]
    interp[string interpolation '$' braces]
    typed[typed call graph: qualified nodes]
    cgram[C grammar + kernel run]
  end

  subgraph SYSTEMS
    glean[Glean]
    souffle[Souffle]
    stackg[Stack Graphs]
    pll[Pruned Landmark Labeling]
    prog[programmable indexer = v5 niche]
  end

  subgraph VISION
    rss[bounded RSS / weak machines]
    lsp[LSP = span queries]
    incr[incremental core: dep-gated rebuild DONE; DRed + incr-SCC next]
    delta[dl --changed = delta entry]
    book[the book + math sub-series]
  end

  fix --> rf
  tarj -->|condenses| cond
  cond --> rf
  cond --> clo
  tarj -->|reused on the RULE graph| st
  strat --> st
  ring -->|one query, many properties| rf
  souffle -->|inspired| ai
  rcg --> endl
  interp --> typed
  typed -->|aliasing 268 to 7| cgram
  dd -.->|orthogonal axis| cond
  dd --> incr
  cond -->|DAG makes counting sound| incr
  pad --> lsp
  pad -->|all calls have DSLs| prog
  rf --> lsp
  glean -.->|model for| clo
  stackg -.->|answer to typed resolution| typed
  prog -.->|vs closed indexers| glean
  rss --> incr
  delta -->|drives| incr
  book --> order
  incr -.-> lsp
```

## Delta log (most recent first)

Format you can string-match: `+n id "label"`, `+e src -> dst "rel"`, `~ note`.

- **t: build incremental core (step 1)** — `+n delta "dl --changed"`; `+e delta -> incr "drives it"`; `~ incr node: dep-gated rebuild LANDED (fc22c16). edit chain A rebuilds only da, db untouched, byte-identical to full. DRed + incr-SCC still pending`.
- **t: conversation graph idea** — `~ dogfood the incrementality thesis on the convo itself`; this file is born.
- **t: DCG / "all calls have DSLs"** — `+n pad "parsing-as-deduction"`; `+e pad -> lsp "span queries"`; `+e pad -> prog "self-hosted grammars"`; `~ DCGs do not port (top-down); use the datalog dual`.
- **t: DD vs condensation** — `+n dd "DBSP/differential"`; `+e dd -> cond "orthogonal axis"`; `+e cond -> incr "DAG makes counting sound"`; `+n incr "incremental core (NEXT)"`.
- **t: can I use Glean** — `+n glean "Glean"`; `+e glean -> clo "model for"`; `+n prog "programmable indexer"`; `+e prog -> glean "vs closed indexers"`.
- **t: SOTA / papers** — `+n stackg`; `+n souffle`; `+n pll`; `+e stackg -> typed "answer to typed resolution"`.
- **t: lattices / order theory** — `+n order`; `+n lat`; `+e order -> lat`; `+e lat -> fix "underpins"`.
- **t: semirings** — `+n ring "reach/shortest/count"`; `+e ring -> rf "one query many properties"`; `+n mind "min-distance"`.
- **t: typed call graph** — `+n interp "$ braces"`; `+n typed "qualified nodes"`; `+e interp -> typed`; `~ max SCC 268 -> 7`.
- **t: C grammar** — `+n cgram "C grammar + kernel"`; `~ 16442 fns, view 1.98s vs Rust 30us`.
- **t: stratification** — `+n strat`; `+n st "stratifier"`; `+e tarj -> st "reused on rule graph"`.
- **(earlier session)** — `+n tarj`; `+n cond`; `+n clo`; `+n rf`; `+n ai`; `+n rcg`; `+n rss`.

## Protocol going forward

Each turn I append a delta block in the format above and fold it into the Mermaid
graph. Each turn is one of: **new** (add a node), **associate** (add an edge to prior
work), or **switch** (a new subgraph / disconnected component). The delta lines are
grep-able, so you can later script the diff/animation off the raw turns if you want.
