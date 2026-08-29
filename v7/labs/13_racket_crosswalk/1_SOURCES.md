# Racket Logic Crosswalk Sources

Research and source-check date: 2026-08-29.

Evidence labels used below:

- `Source receipt` means a current official manual, package-catalog record, or upstream repository link.
- `Local receipt` means a command run on this machine. Local receipts describe this installation and do not establish cross-platform behavior.
- Package catalog records expose source commits rather than semantic package versions for the packages covered here.

## Current Racket and package sources

| Area | Source receipt | Version, commit, or current claim |
| --- | --- | --- |
| Racket release | [Racket 9.3 release page](https://download.racket-lang.org/releases/9.3/) and [9.3 release announcement](https://download.racket-lang.org/v9.3.html) | Racket 9.3, released August 2026; the local runtime reports CS 9.3. |
| `datalog` language and API | [Datalog manual](https://docs.racket-lang.org/datalog/), [Datalog module language](https://docs.racket-lang.org/datalog/datalog.html), [Racket interoperability](https://docs.racket-lang.org/datalog/interop.html), [parenthetical language](https://docs.racket-lang.org/datalog/Parenthetical_Datalog_Module_Language.html) | The package defines function-free Horn clauses, requires safe rule heads, uses tabling of intermediate results, supports `make-theory`, `datalog`, `!`, `~`, `:-`, and `?`, and documents terminating pure Datalog queries. `datalog/sexp` is a language module in the `datalog` package. |
| `datalog` package source | [Racket package catalog](https://pkgs.racket-lang.org/package/datalog), [upstream repository](https://github.com/racket/datalog/tree/3a30258c7a11f6f1836f52590e26542c11865714) | Catalog source commit `3a30258c7a11f6f1836f52590e26542c11865714`; Apache-2.0 OR MIT; catalog record lists `datalog/sexp/lang.rkt`. |
| Racklog language and API | [Racklog manual](https://docs.racket-lang.org/racklog/), [predicates and rules](https://docs.racket-lang.org/racklog/predicates.html), [backtracking](https://docs.racket-lang.org/racklog/backtracking.html), [unification](https://docs.racket-lang.org/racklog/unification.html), [variable operations](https://docs.racket-lang.org/racklog/lv-manip.html) | Racklog is an embedding of Prolog-style logic in Racket. `%which` returns one answer, `%more` requests subsequent answers, `%rel` defines clauses, `%assert!` and `%assert-after!` add clauses, and `use-occurs-check?` defaults to `#f`. The manual says Racklog has no predicate for retracting assertions. |
| Racklog package source | [Racket package catalog](https://pkgs.racket-lang.org/package/racklog), [upstream repository](https://github.com/racket/racklog/tree/66c30f8c920bf442025acbcf7fbeefa0536f464c) | Catalog source commit `66c30f8c920bf442025acbcf7fbeefa0536f464c`; Apache-2.0 OR MIT; catalog record lists `racklog` as a main-distribution package. |
| Baseline miniKanren package | [Racket miniKanren repository](https://github.com/takikawa/minikanren) | The repository describes a package version of Dan Friedman’s original miniKanren and directs practical users to cKanren. The repository README gives `raco pkg install minikanren`. |
| cKanren | [cKanren package catalog record](https://pkgs.racket-lang.org/package/cKanren), [upstream repository](https://github.com/calvis/cKanren/tree/8714bdd442ca03dbf5b1d6250904cbc5fd275e68) | Catalog source commit `8714bdd442ca03dbf5b1d6250904cbc5fd275e68`; catalog modules include `miniKanren.rkt`, `constraints.rkt`, `constraint-store.rkt`, `neq.rkt`, `numbero.rkt`, `symbolo.rkt`, `absento.rkt`, and finite-domain modules. The catalog record reports successful compilation, failing tests, and missing license metadata. |
| Hosted miniKanren | [hosted-minikanren package catalog record](https://pkgs.racket-lang.org/package/hosted-minikanren), [upstream repository](https://github.com/michaelballantyne/hosted-minikanren/tree/867755c61acd064a07dfcfd0849b01a70f315bed) | Catalog source commit `867755c61acd064a07dfcfd0849b01a70f315bed`; package description is an optimizing compiler implementation of miniKanren; license MIT; catalog record reports dependency problems and failing tests in addition to successful compilation. |
| Rosette | [Rosette getting started guide](https://docs.racket-lang.org/rosette-guide/ch_getting-started.html), [Rosette essentials](https://docs.racket-lang.org/rosette-guide/ch_essentials.html), [package catalog record](https://pkgs.racket-lang.org/package/rosette), [upstream repository](https://github.com/emina/rosette/tree/29808a02d2a2c25a6824bfed5df32cc263dea734) | Rosette supplies symbolic values, assertions, assumptions, solver-aided queries, synthesis, and verification. Catalog source commit `29808a02d2a2c25a6824bfed5df32cc263dea734`; the catalog record reports successful compilation and passing tests. |
| Syntax objects and phases | [Racket syntax model](https://docs.racket-lang.org/reference/syntax-model.html), [reader reference](https://docs.racket-lang.org/reference/reader.html), [reader helpers](https://docs.racket-lang.org/syntax/reader-helpers.html) | Reading in `read-syntax` mode produces syntax objects with source locations and lexical information. Expansion uses scope sets and phase levels. `free-identifier=?` and `bound-identifier=?` compare binding identity. |
| `#lang` implementation | [Racket Guide: `#lang` syntax](https://docs.racket-lang.org/guide/hash-lang_syntax.html), [Guide: `#lang reader`](https://docs.racket-lang.org/guide/hash-lang_reader.html), [Guide: `syntax/module-reader`](https://docs.racket-lang.org/guide/syntax_module-reader.html) | A language resolves to a reader module, which supplies `read` and `read-syntax`; `syntax/module-reader` supplies the standard reader protocol and supports custom readers. |
| Executable creation | [Racket `raco exe` manual](https://docs.racket-lang.org/raco/exe.html) | `raco exe` embeds a module and its reachable required modules in an executable. Dynamic `eval`, `load`, and `dynamic-require` dependencies need explicit packaging treatment. Language readers used only through dynamic `#lang` use require `++lang`. |
| Distribution | [Racket `raco distribute` manual](https://docs.racket-lang.org/raco/exe-dist.html) | `raco distribute` combines a stand-alone executable with required shared libraries and runtime files for machines using the same operating system. On macOS, non-GUI executables go under `bin` and frameworks go under `lib`. |
| Graph support | [graph-lib package catalog record](https://pkgs.racket-lang.org/package/graph-lib), [upstream graph repository](https://github.com/stchang/graph/tree/9d77ab184e26f4f3c917c7bd49eda2e980a24fae) | Catalog source commit `9d77ab184e26f4f3c917c7bd49eda2e980a24fae`; the package record lists graph construction and graph algorithms. |

## SWI source receipts used for the comparison

The SWI rows use the same facility boundary as the local comparison reference. The links are SWI’s current manuals for the corresponding mechanisms.

| SWI facility | Source receipt |
| --- | --- |
| Tabling and SLG-style evaluation | [SWI tabling manual](https://www.swi-prolog.org/pldoc/man?section=tabling) |
| Constraint libraries | [SWI constraint logic programming manual](https://www.swi-prolog.org/pldoc/man?section=clp) |
| Constraint Handling Rules | [SWI CHR manual](https://www.swi-prolog.org/pldoc/man?section=chr) |
| Saved states | [SWI saved states manual](https://www.swi-prolog.org/pldoc/man?section=saved-states) |
| Foreign threads and engines | [SWI foreign-thread and engine manual](https://www.swi-prolog.org/pldoc/man?section=foreignthread) |

## Local receipts

The brief contains an outdated baseline sentence that places Racket outside `PATH`. The current local installation was used without package installation or global configuration changes.

```text
$ command -v racket
/opt/homebrew/bin/racket

$ racket --version
Welcome to Racket v9.3 [cs].

$ raco pkg show datalog
Installation-wide: [none]
User-specific for installation "9.3": Package datalog ...

$ raco pkg show racklog
Installation-wide: [none]
User-specific for installation "9.3": Package racklog ...
```

The installed package metadata under `/Users/chrishafley/Library/Racket/9.3/pkgs/` records the same upstream commits cited above and says `built "9.3"` in each package’s `info.rkt`.

Bounded behavior probes:

| Command | Outcome |
| --- | --- |
| `just runtime-shootout-smoke` from `v7/` | Exit 0. SBCL, SWI-Prolog, and Racket 9.3 each returned the expected chain count `6` and ring count `16` at `N=4`. |
| `racket v7/labs/18_runtime_shootout/3_racket.rkt chain 4` and `ring 4` | Exit 0. Racket 9.3 returned closure counts `6` and `16`. |
| `racket /Users/chrishafley/Library/Racket/9.3/pkgs/datalog/tests/paren-examples/path.rkt` | Exit 0. `#lang datalog/sexp` returned the 16 path facts for a four-node ring. |
| `racket /Users/chrishafley/Library/Racket/9.3/pkgs/racklog/tests/lang/ancestor.rkt` | Exit 0. Racklog returned six ancestor facts for the documented finite ancestor program. |
| `racket -e '(require racklog) ... (use-occurs-check? #t) ...'` | Exit 0. The bounded cyclic-unification query returned `(#t #f)`, where the first value records the enabled policy and the second records query failure. |
| `racket -e '(require datalog) ...'` with `path(a,Y)` before and after `~ (edge b c)` | Exit 0. The first query returned `Y=b,c`; after retraction it returned `Y=b`. |
| `raco exe -o /private/tmp/.../dl7-racket-arm v7/labs/18_runtime_shootout/3_racket.rkt` followed by `chain 4` | Exit 0. The executable ran the Racket Datalog arm and reported closure count `6`; the temporary Mach-O file was `11,774,987` bytes. |
| `raco distribute ...` followed by the packaged `ring 4` executable | The first attempt hit a write-permission error while patching the copied Mach-O executable. After adding owner write permission to the temporary executable, distribution completed and the packaged executable returned closure count `16`. |

The supplied [runtime-shootout results](../18_runtime_shootout/5_RESULTS.md) are also a local receipt. At `N=48`, the recorded Racket medians are 340.000 ms process startup, 495.746 ms chain closure, and 1966.985 ms ring closure; the recorded counts are 1128 and 2304. Those measurements compare the selected native routes and retain the algorithm differences specified by the shootout README.

## Source limitations

No package installation was performed for cKanren, baseline miniKanren, hosted miniKanren, or Rosette. Their current catalog records establish package metadata and source coordinates; their behavior remains `absent-from-probe` in this lab. The local `raco pkg catalog-show` command also attempted to contact `download.racket-lang.org` and failed DNS resolution in the sandbox. The official catalog and documentation pages above supplied the current source receipts.
