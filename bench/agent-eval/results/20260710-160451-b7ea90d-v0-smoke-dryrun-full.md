# Agent eval harness — v0-smoke

Score window: +/- 2 lines. Rows: 408. Tasks: 34. Cells: haiku-bash, haiku-dl, sonnet-bash, sonnet-dl.

## Headline (median-of-reps C2 locate rate)

| cell | median locate rate | raw per-rep rate | parse-failure rate | tasks | reps |
|---|---|---|---|---|---|
| haiku-bash | 52.94% | 56.86% | 20.59% | 34 | 3 |
| haiku-dl | 50.00% | 54.90% | 23.53% | 34 | 3 |
| sonnet-bash | 50.00% | 46.08% | 27.45% | 34 | 3 |
| sonnet-dl | 52.94% | 54.90% | 26.47% | 34 | 3 |

## Cost per solved task

| cell | total cost (usd) | solved (median) | cost / solved |
|---|---|---|---|
| haiku-bash | $0.1020 | 18 | $0.0057 |
| haiku-dl | $0.1020 | 17 | $0.0060 |
| sonnet-bash | $0.1020 | 17 | $0.0060 |
| sonnet-dl | $0.1020 | 18 | $0.0057 |

## Where dl lost

Tasks the bash-only cell solved (median) but the matching +dl cell did not, same model. An empty table under a model heading is a real finding (dl never lost for that model on this run), not an omission.

### haiku-bash vs haiku-dl

| task_id |
|---|
| c2-duplicate-sym-01 |
| c2-duplicate-sym-02 |
| c2-duplicate-sym-09 |
| c2-export-orphan-00 |
| c2-export-orphan-01 |
| c2-export-orphan-05 |
| c2-export-orphan-06 |
| c2-import-break-06 |
| c2-off-by-one-02 |

### sonnet-bash vs sonnet-dl

| task_id |
|---|
| c2-duplicate-sym-03 |
| c2-duplicate-sym-06 |
| c2-duplicate-sym-08 |
| c2-export-orphan-08 |
| c2-off-by-one-02 |

## Tool bugs found

Not wired in S1 (diagnose.dl + query_log join is stage S3 per the plan); this section is a placeholder so the report template shape is stable before S3 fills it.
