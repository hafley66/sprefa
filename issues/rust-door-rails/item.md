---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# Rust-door rails and goldens whose hosts now stop by name after the shell deletion

## Description

dl/dataflow/report_extract.dl6 (5 hosts, no sidecar), dl/hotpath/serde-default-rail.dl6 (6 hosts, sidecar empty), dl/hotpath/prolog-hotpath-rails.dl6 (files row missing); tsv2 goldens driving emit_rust_harness --live-hosts over git_* / dep_crawl_* / repo_files (multirepo_crawl 5,8,11; scip_combo 8; cpg_taint_walk 3; crawl-bench.sh) have no executor. Decide per host: link (soopy, extract) or retire the gate. tsv2 paused, so the goldens retire unless a Rust-door consumer exists.
