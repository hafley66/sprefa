# Commercial Common Prolog Sources

Research and source-check date: 2026-08-29.

## Evidence labels

- `Source receipt` means a vendor manual, release page, platform matrix, license page, or vendor example checked on the web.
- `Local receipt` means a command run in this repository. This lab has no local product receipt.
- `Undocumented` means the checked vendor sources do not establish the feature. It is not a claim about hidden implementation behavior.
- `Unavailable` is used only where a vendor source explicitly excludes a feature or edition.

## Product identity and current versions

| Product | Current host release | Prolog-specific source | Identity boundary |
| --- | --- | --- | --- |
| Allegro Prolog | Allegro CL 11.0 | Allegro Prolog User Documentation, version 1.1.2, page labeled `Allegro CL version 10.1` | Allegro Prolog is the Prolog extension inside Allegro Common Lisp. AllegroGraph is a separate graph database product and is excluded from this lab. |
| LispWorks Common Prolog | LispWorks 8.1.2, the current patch release for the 8.1 line | KnowledgeWorks and Prolog User Guide for LispWorks 8.1, including Appendix A Common Prolog | Common Prolog is the backward-chaining logic system. KnowledgeWorks is the separate rule and knowledge-base product that includes and extends Common Prolog. |
| KnowledgeWorks | LispWorks 8.1.2 host, KnowledgeWorks included in Enterprise Edition | KnowledgeWorks product page and the 8.1 KnowledgeWorks and Prolog User Guide | KnowledgeWorks adds an OPS-compatible forward chainer, CLOS object base, backward-chaining integration, contexts, conflict resolution, RETE implementation, and truth maintenance. |

The current Allegro CL 11.0 documentation table of contents still lists Allegro Prolog as an add-on document, but the linked Prolog document identifies itself as Allegro CL 10.1 and Allegro Prolog 1.1.2. No newer Prolog-specific version identifier was found in the checked Franz sources. Claims about Allegro Prolog below are therefore documented against that linked manual and are marked as current-version-unverified where relevant.

## Allegro CL and Allegro Prolog

| Area | Source receipt | Recorded fact |
| --- | --- | --- |
| Product page | [Allegro Prolog product page](https://franz.com/products/prolog/index.lhtml) | Franz describes Allegro Prolog as an integrated extension to Common Lisp, based on Peter Norvig's implementation, compiling each functor/arity into one optimized Lisp function, and aiming for essentially cons-free execution. |
| Prolog manual | [Allegro Prolog User Documentation](https://franz.com/support/documentation/prolog.html) | The manual documents S-expression syntax, `?`-prefixed variables, `prolog` and `?-` query interfaces, compiled predicates, built-ins, Lisp interoperation, leash tracing, dynamic databases, and a zebra example. |
| Current host release | [Allegro CL 11.0 product page](https://franz.com/products/allegrocl/), [11.0 documentation contents](https://examples.franz.com/support/documentation/contents.html) | The current Franz product pages identify Allegro CL 11.0. The 11.0 contents list Allegro Prolog, but the linked Prolog manual's version header remains 10.1/1.1.2. |
| Release and platforms | [Allegro CL 11.0 release notes](https://examples.franz.com/support/documentation/release-notes-11.0.html) | Host platforms documented for 11.0 include Apple macOS 12 on Apple Silicon and x86-64, Linux x86-64 with glibc 2.17, Linux ARM64 with glibc 2.17 or Amazon Linux ARM v8.1 with glibc 2.26, Windows x86-64, Windows 32-bit, and FreeBSD 13 for SMP. |
| Express downloads | [Allegro CL Express downloads](https://franz.com/downloads/clp/download), [Express information](https://franz.com/products/express/) | The current Express page identifies ACL 11.0. It lists 32-bit Windows, x86-64 Linux, Apple Silicon macOS, and additional x86-64 macOS, ARM Linux, and FreeBSD downloads. The page does not establish that Allegro Prolog is included in Express. |
| License file | [Allegro CL installation guide](https://examples.franz.com/support/documentation/installation.html), [Allegro CL startup and license file](https://examples.franz.com/support/documentation/startup.html) | Allegro CL requires a `devel.lic` license file. The startup documentation says paid licenses have no expiration date in the sample model, while Express licenses have an expiration date. |
| Free evaluation | [Allegro CL Free Express information](https://franz.com/products/express/), [Express installation guide](https://examples.franz.com/support/documentation/express-installation.html) | The latest Express version is 11.0. Franz states that the June 2025 Express license expires 2027-06-15 and describes Express as free software for students, hobbyists, and prospective customers with a heap limitation. |
| Commercial restrictions | [Franz Software License Agreement](https://franz.com/products/licensing/FSLA.pdf) | The agreement restricts evaluation use to evaluation purposes and excludes commercial use, services or products for others, university-sanctioned research projects, and government use. The agreement says Professional licenses do not authorize runtime distribution; commercial runtime distribution requires the applicable separate license. |
| Runtime delivery | [Allegro CL Runtime documentation](https://examples.franz.com/support/documentation/runtime.html) | Enterprise and Enterprise Platinum licenses include standard runtime creation. Professional does not include a runtime application license. Dynamic and Partner runtimes require separately purchased rights. The manual documents `generate-application` and related delivery routes for the host runtime. |
| Vendor examples | [Allegro Prolog manual examples](https://franz.com/support/documentation/prolog.html) | The manual includes `append/3` queries, `leash` output, Lisp-to-Prolog and Prolog-to-Lisp examples, recorded databases, generators, standard-object slot access, and a zebra benchmark. |

## LispWorks Common Prolog and KnowledgeWorks

| Area | Source receipt | Recorded fact |
| --- | --- | --- |
| Current release | [LispWorks 8.1 release announcement](https://www.lispworks.com/news/news42.html), [current patches](https://www.lispworks.com/downloads/patch-selection.html) | LispWorks 8.1 was released on 2025-03-03. The current patch page identifies 8.1.2 as the current version for Hobbyist, HobbyistDV, Professional, and Enterprise editions. |
| Host platforms | [LispWorks 8.1 documentation page](https://www.lispworks.com/documentation/), [8.1 release announcement](https://www.lispworks.com/news/news42.html), [8.1.2 platform patches](https://www.lispworks.com/downloads/patch-selection.html) | The documentation and patch pages list Macintosh, Windows, x86/x86_64 Linux, ARM Linux, x86/x64 Solaris, and FreeBSD builds. The release announcement also identifies Android and iOS runtime products. |
| Feature availability | [LispWorks feature matrix](https://www.lispworks.com/products/features.html) | The 8.1 feature matrix places “KnowledgeWorks and Prolog” in the Enterprise column and shows support across the listed desktop platform columns. |
| KnowledgeWorks identity | [KnowledgeWorks product page](https://www.lispworks.com/products/knowledgeworks.html) | KnowledgeWorks is an integrated knowledge-based-system environment with an OPS-compatible forward chainer, Prolog-compatible backward chainer, CLOS object representation, contexts, conflict resolution, logical dependencies, truth maintenance, and multi-platform delivery. |
| Common Prolog guide | [KnowledgeWorks and Prolog User Guide, 8.1 PDF](https://www.lispworks.com/documentation/pdf/lw81/kw-w-8-1.pdf), [HTML contents](https://www.lispworks.com/documentation/lw80/kw-w/kw-contents.htm) | Appendix A documents Common Prolog syntax, `defrel`, modes and clause indexing, the query listener, Lisp calls, Lisp integration, debugging, logic macros, DCGs, Edinburgh syntax, built-ins, and adding built-ins. The PDF also includes KnowledgeWorks rules and examples. |
| Common Prolog introduction | [Appendix A.1 Common Prolog](https://www.lispworks.com/documentation/lw80/kw-w/kw-prolog-1.htm) | Common Prolog is described as a logic system within Common Lisp. Predicates compile into Lisp functions, the implementation is based loosely on the WAM, and KnowledgeWorks loading also loads Common Prolog. |
| License and evaluation | [Evaluation licenses](https://www.lispworks.com/buy/evaluation.html), [LispWorks FAQ](https://www.lispworks.com/support/faq.html) | Evaluation licenses are supplied as downloads and last one month. The request must identify platform, edition, bitness, number of licenses, and intended use. Hobbyist and HobbyistDV are restricted to individual non-commercial and non-academic use. Each desktop platform is separately licensed. |
| Free Personal Edition | [LispWorks Personal Edition](https://www.lispworks.com/downloads/index.html) | Personal Edition 8.1.2 is free, has a heap limit and a five-hour session limit, and does not provide `save-image`, `deliver`, or initialization files. KnowledgeWorks and Prolog are explicitly excluded. |
| Runtime delivery | [LispWorks 8.1 release announcement](https://www.lispworks.com/news/news42.html), [LispWorks FAQ](https://www.lispworks.com/support/faq.html), [KnowledgeWorks introduction](https://www.lispworks.com/documentation/lw81/kw-m/kw-introduction-1.htm) | LispWorks states that Professional and Enterprise end-user applications are royalty-free. KnowledgeWorks delivery is documented for Enterprise Edition and the separately licensed iOS and Android runtime products. Personal Edition cannot deliver images. |
| Vendor examples | [Common Prolog guide](https://www.lispworks.com/documentation/pdf/lw81/kw-w-8-1.pdf), [KnowledgeWorks rules](https://www.lispworks.com/documentation/lw81/kw-w/kw-rules-1.htm) | The guide includes `append/3`, `reverse/2`, factorial, `logic`, `findall`, `with-prolog` palindrome code, `defrelmacro`, `defgrammar` sentence examples, Edinburgh translation, tutorial rules, forward chaining, backward chaining, and truth-maintenance examples. |

## Source limitations

1. No Allegro CL, Allegro Prolog, LispWorks, Common Prolog, or KnowledgeWorks executable is present in the local toolchain. No installation, license activation, product command, or product code was run.
2. Allegro Prolog's public manual is versioned 1.1.2 and labeled for Allegro CL 10.1. The current Allegro CL host page and 11.0 contents page do not provide a separate 11.0 Prolog manual or Prolog release note in the checked sources.
3. The vendor pages document product surfaces, examples, and implementation descriptions. They do not provide source repositories or commits for either commercial Prolog implementation.
4. Absence of a word or predicate from a checked manual is recorded as `undocumented`. It is not converted into an implementation claim.
5. The checked documents establish Common Prolog and KnowledgeWorks separately. KnowledgeWorks forward chaining, RETE maintenance, CLOS objects, and truth maintenance are reported as KnowledgeWorks facilities and are not counted as Common Prolog tabling, CLP, or SWI database semantics.
