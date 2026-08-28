# Lab 15 Sources: Commercial Common Lisp Prolog Products

Research date: 2026-08-28. No local execution of either product; both are
absent from the local toolchain (`sbcl 2.6.7` and `swipl 10.0.2` are installed,
no Allegro CL, no LispWorks). Every claim below is product documentation or
product web page content, fetched and archived locally under
`/private/tmp/sprefa-v7-lab15/`.

## Franz Inc. Allegro Prolog (inside Allegro CL)

| Item | Value | Source |
| --- | --- | --- |
| Current product version | Allegro CL 11.0 | [A] |
| Prolog component version | Allegro Prolog 1.1.2 (doc header "Thu Dec 12 2013 by smh", release 1.1.2 continues minor bugfix/feature sequence) | [B] |
| Doc version caveat | The "current" Prolog documentation URL served the Allegro CL 10.1 doc set in the newest obtainable snapshot (2023-02-12); franz.com returned HTTP 522 (origin timeout) on every direct fetch on 2026-08-28. The ACL 11.0-era Prolog chapter was not separately obtainable. Content claims are therefore documented against the 10.1-era Prolog doc. | [B] |
| Loading | `(require :prolog)`; symbols exported from the `prolog` package; optional `(use-package :prolog)` with `(shadowing-import '(prolog:==))` when `cg` is present | [B] |
| Editions containing Prolog | Prolog is listed as part of the Allegro CL product ("Allegro Prolog: A classic AI programming language in Allegro CL ... execution speed in excess of 1 Mlips and running essentially cons free"); `pcache` (Prolog as AllegroCache query language) ships with AllegroCache | [A] |
| Free tier | Allegro CL 11.0 Free Express Edition, download gated by a survey form and an Express license agreement; includes AllegroCache; limited technical support | [C] |
| Commercial licensing | Franz Software License Agreement; Corporate End User License for for-profit/government use; Value Added Reseller License for resold applications; non-commercial environments (learning, unpaid applications, Franz-approved evaluations) licensed separately; pricing by quote (sales@franz.com) | [D] |
| Platforms (ACL 11.0) | Linux x86-64 (glibc 2.17), Linux ARM v8.1 Amazon Graviton (glibc 2.26), Linux ARM v8 RHEL/CentOS, macOS Apple Silicon, macOS x86-64, Windows x86 32-bit, Windows x86-64, FreeBSD x86-64 (SMP); non-SMP and SMP variants except FreeBSD | [A] |
| Deployment | "Application delivery as a DLL or stand alone image" listed under Runtime | [A] |
| Related runtime | AllegroCache object database; Prolog reasons over it through the `pcache` module's `db` predicate with first-slot indexing | [B] |

Sources:

- [A] Franz Inc., "Allegro Common Lisp" product page, https://franz.com/products/allegro-common-lisp/ . Live site unreachable (HTTP 522) on 2026-08-28; content read from the Wayback Machine snapshot 2026-08-24 (`http://web.archive.org/web/20260824165707/...`). Local copy: `/private/tmp/sprefa-v7-lab15/acl-product-2026.html` (uncompressed `acl-product.zst`).
- [B] Franz Inc., "Prolog: Allegro Prolog", Allegro CL documentation, https://franz.com/support/documentation/current/doc/prolog.html . Live site HTTP 522 on 2026-08-28; content read from Wayback snapshot 2023-02-12 (`http://web.archive.org/web/20230212205119id_/...`), which identifies itself as "Allegro CL version 10.1". Local copies: `allegro-prolog-wb.html`, `allegro-prolog.txt`.
- [C] Franz Inc., "Allegro Common Lisp Free Express Edition Download", https://franz.com/downloads/clp/survey , Wayback snapshot 2026-03-24. Local copy: `free.html`.
- [D] Franz Inc., "Licensing Allegro CL", https://franz.com/products/licensing/ , Wayback snapshot 2026-06-12. Local copy: `lic.html`.

## LispWorks Ltd. Common Prolog and KnowledgeWorks

| Item | Value | Source |
| --- | --- | --- |
| Current product version | LispWorks 8.1; KnowledgeWorks and Prolog User Guide dated 18 Feb 2025 (Macintosh version footer) | [E] |
| Component identity | Common Prolog is a Lisp-hosted Prolog compiler shipped with KnowledgeWorks; loadable alone with `(require "prolog")`; loaded as part of KnowledgeWorks. Backward chainer is "an extended Prolog based on the Warren Abstract Machine"; each Prolog clause compiles into a function with continuation-passing control flow | [E] [F] |
| Edition boundary | KnowledgeWorks (and therefore Common Prolog) is included with LispWorks Enterprise Edition only; Professional/Hobbyist/HobbyistDV/Personal do not include it | [E] [F] |
| Personal Edition | Free; full compiler and IDE but "limit program size and duration", no application delivery | [E] |
| Evaluation license | One month, per platform/edition/bitness on request by email; download supplied after license | [G] |
| Prices (commercial, USD, per user, dated 3 March 2025) | Enterprise 64-bit: macOS/Windows 64-bit/Linux x86-64/ARM Linux/FreeBSD/Solaris $4,500 (+maintenance $1,125/yr); Enterprise 32-bit Windows/Linux/Solaris/FreeBSD $4,500; Professional 32-bit $1,500, 64-bit $3,000; upgrades Professional to Enterprise $3,000 (32-bit paths) / $1,500 (64-bit to 64-bit); maintenance renewal 32-bit $375/$1,125, 64-bit $750/$1,125. Hobbyist and HobbyistDV are lower-priced individual licenses; Personal is free | [H] |
| Platforms | Windows 32/64-bit, macOS (64-bit), x86/x86_64 Linux, ARM Linux 32/64-bit, FreeBSD 32/64-bit, x86/x64 Solaris; source compatible across platforms | [E] |
| Deployment | `deliver` function generates runtime executables and libraries; available in Professional, Enterprise, HobbyistDV; omitted from Hobbyist and Personal ("runtimes cannot be generated" from Hobbyist; Personal "does not support application delivery"); no runtime license fees for Professional/Enterprise-developed applications | [E] |
| Debugging | 4-port model (call/exit/redo/fail), trace, spy points, leashing, interactive debug command loop (creep/skip/leap/break/display/quit/retry/fail/abort); KnowledgeWorks adds spy windows, monitor windows, single stepping in the IDE | [I] |

Sources:

- [E] LispWorks Ltd., "KnowledgeWorks and Prolog User Guide", LispWorks 8.1 documentation, https://www.lispworks.com/documentation/lw81/kw-m/kw.htm (fetched live 2026-08-28; 135-page mirror under `/private/tmp/sprefa-v7-lab15/kw-*.htm`, flattened text `kw-full.txt`).
- [F] LispWorks Ltd., "KnowledgeWorks" product page, https://www.lispworks.com/products/knowledgeworks.html (fetched live 2026-08-28; `lw-kw-product.html`).
- [G] LispWorks Ltd., "Evaluation licences", https://www.lispworks.com/buy/evaluation.html (fetched live 2026-08-28; `buy-evaluation.html`).
- [H] LispWorks Ltd., "LispWorks Prices for Commercial Users", https://www.lispworks.com/buy/prices-1c.html , prices dated 3 March 2025 (fetched live 2026-08-28; `prices-1c.html`).
- [I] LispWorks Ltd., editions and delivery statements, https://www.lispworks.com/products/lispworks.html (fetched live 2026-08-28; `products-lispworks.html`).

## Verification status

- Documented: everything cited above, from vendor manuals and vendor web pages.
- Unavailable for local verification: all behavior. Neither Allegro CL nor LispWorks is installed; no trial was installed (the assignment forbids installing products outside temp lab space, and both require license-gated downloads). Every capability statement in `2_CAPABILITIES.md` carries a doc citation and none carries a local receipt.
- The Allegro Prolog chapter is the ACL 10.1 edition of the manual (2023 snapshot); ACL 11.0-era changes to Allegro Prolog are undocumented in the obtainable material and are marked as gaps where relevant.
