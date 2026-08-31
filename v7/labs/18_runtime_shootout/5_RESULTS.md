# Native-logic runtime shootout results

- Generated: 2026-08-29 11:59:14 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 24 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| racket | 9.3 | 340.000 | 161873920 |
| sbcl | 2.6.7 | 20.000 | 41517056 |
| swi | 10.0.2 | 10.000 | 8650752 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| racket | chain | 47 | 1128 | 0.25799999999998136 | 495.746 | 820.000 | 152829952 |
| racket | ring | 48 | 2304 | 0.30299999999999727 | 1966.9850000000001 | 2350.000 | 152764416 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.126 | 30.000 | 46743552 |
| sbcl | ring | 48 | 2304 | 0.001 | 0.258 | 30.000 | 46940160 |
| swi | chain | 47 | 1128 | 0 | 1 | 20.000 | 11288576 |
| swi | ring | 48 | 2304 | 0 | 3 | 20.000 | 11124736 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":20.000,"peak_rss_bytes":41156608}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":20.000,"peak_rss_bytes":41222144}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":20.000,"peak_rss_bytes":41336832}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":20.000,"peak_rss_bytes":41254912}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":20.000,"peak_rss_bytes":41517056}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":10.000,"peak_rss_bytes":8093696}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":10.000,"peak_rss_bytes":8175616}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":10.000,"peak_rss_bytes":8175616}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":10.000,"peak_rss_bytes":7716864}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":10.000,"peak_rss_bytes":8650752}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":330.000,"peak_rss_bytes":161742848}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":340.000,"peak_rss_bytes":161677312}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":330.000,"peak_rss_bytes":161185792}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":410.000,"peak_rss_bytes":161873920}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":410.000,"peak_rss_bytes":161742848}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.130,"kind":"closure","repetition":1,"process_ms":30.000,"peak_rss_bytes":46743552}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46415872}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.002,"closure_ms":0.126,"kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.002,"closure_ms":0.124,"kind":"closure","repetition":4,"process_ms":30.000,"peak_rss_bytes":46399488}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.125,"kind":"closure","repetition":5,"process_ms":30.000,"peak_rss_bytes":46071808}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.254,"kind":"closure","repetition":1,"process_ms":30.000,"peak_rss_bytes":46940160}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.002,"closure_ms":0.321,"kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.251,"kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":46645248}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.002,"closure_ms":0.261,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46596096}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.258,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46776320}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":11288576}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":9469952}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":3,"process_ms":40.000,"peak_rss_bytes":11010048}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":10698752}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":10371072}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":10567680}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":11042816}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":10403840}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":10043392}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":4,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":11124736}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.28600000000000136,"closure_ms":495.746,"kind":"closure","repetition":1,"process_ms":820.000,"peak_rss_bytes":152485888}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.23799999999999955,"closure_ms":488.62199999999996,"kind":"closure","repetition":2,"process_ms":800.000,"peak_rss_bytes":150437888}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.3380000000000223,"closure_ms":849.0379999999999,"kind":"closure","repetition":3,"process_ms":1200.000,"peak_rss_bytes":152600576}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2540000000000191,"closure_ms":438.95799999999997,"kind":"closure","repetition":4,"process_ms":750.000,"peak_rss_bytes":152797184}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.25799999999998136,"closure_ms":527.666,"kind":"closure","repetition":5,"process_ms":860.000,"peak_rss_bytes":152829952}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.5010000000000332,"closure_ms":2349.9889999999996,"kind":"closure","repetition":1,"process_ms":2730.000,"peak_rss_bytes":152764416}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.27799999999996317,"closure_ms":1802.0200000000002,"kind":"closure","repetition":2,"process_ms":2190.000,"peak_rss_bytes":152535040}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.27199999999999136,"closure_ms":1770.518,"kind":"closure","repetition":3,"process_ms":2080.000,"peak_rss_bytes":150241280}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.30299999999999727,"closure_ms":1966.9850000000001,"kind":"closure","repetition":4,"process_ms":2350.000,"peak_rss_bytes":152371200}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.30299999999999727,"closure_ms":2035.7220000000002,"kind":"closure","repetition":5,"process_ms":2400.000,"peak_rss_bytes":152600576}
```
