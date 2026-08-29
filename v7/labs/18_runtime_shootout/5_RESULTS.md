# Native-logic runtime shootout results

- Generated: 2026-08-29 11:50:13 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 40 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| racket | 9.3 | 340.000 | 161824768 |
| sbcl | 2.6.7 | 20.000 | 41385984 |
| swi | 10.0.2 | 10.000 | 8683520 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| racket | chain | 47 | 1128 | 0.8139999999999645 | 986.332 | 1440.000 | 152764416 |
| racket | ring | 48 | 2304 | 0.5690000000000168 | 3493.496 | 3820.000 | 152993792 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.124 | 30.000 | 46661632 |
| sbcl | ring | 48 | 2304 | 0.002 | 0.252 | 30.000 | 47185920 |
| swi | chain | 47 | 1128 | 0 | 2 | 30.000 | 11780096 |
| swi | ring | 48 | 2304 | 0 | 3 | 30.000 | 12189696 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":10.000,"peak_rss_bytes":41385984}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":20.000,"peak_rss_bytes":41172992}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":20.000,"peak_rss_bytes":41140224}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":20.000,"peak_rss_bytes":41189376}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":20.000,"peak_rss_bytes":41205760}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":10.000,"peak_rss_bytes":8306688}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":20.000,"peak_rss_bytes":7798784}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":10.000,"peak_rss_bytes":8568832}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":10.000,"peak_rss_bytes":8683520}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":10.000,"peak_rss_bytes":8323072}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":320.000,"peak_rss_bytes":161759232}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":370.000,"peak_rss_bytes":161808384}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":450.000,"peak_rss_bytes":161742848}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":340.000,"peak_rss_bytes":161824768}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":290.000,"peak_rss_bytes":161562624}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.002,"closure_ms":0.125,"kind":"closure","repetition":1,"process_ms":40.000,"peak_rss_bytes":46661632}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.124,"kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":46350336}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.121,"kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46497792}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.121,"kind":"closure","repetition":5,"process_ms":30.000,"peak_rss_bytes":46333952}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.002,"closure_ms":0.252,"kind":"closure","repetition":1,"process_ms":40.000,"peak_rss_bytes":47022080}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.004,"closure_ms":0.567,"kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":46972928}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.249,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.002,"closure_ms":0.250,"kind":"closure","repetition":4,"process_ms":30.000,"peak_rss_bytes":46530560}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.005,"closure_ms":0.642,"kind":"closure","repetition":5,"process_ms":40.000,"peak_rss_bytes":47185920}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":30.000,"peak_rss_bytes":11059200}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":11780096}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":10960896}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":4,"process_ms":40.000,"peak_rss_bytes":11403264}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":5,"process_ms":30.000,"peak_rss_bytes":11124736}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":4,"kind":"closure","repetition":1,"process_ms":40.000,"peak_rss_bytes":12189696}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":11059200}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":1,"closure_ms":2,"kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":10977280}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":4,"process_ms":30.000,"peak_rss_bytes":11354112}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":7,"kind":"closure","repetition":5,"process_ms":50.000,"peak_rss_bytes":11550720}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.24799999999999045,"closure_ms":986.332,"kind":"closure","repetition":1,"process_ms":1440.000,"peak_rss_bytes":152764416}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":3.2449999999999477,"closure_ms":853.9270000000001,"kind":"closure","repetition":2,"process_ms":1230.000,"peak_rss_bytes":150274048}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.8139999999999645,"closure_ms":1529.907,"kind":"closure","repetition":3,"process_ms":2270.000,"peak_rss_bytes":149258240}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":1.752999999999929,"closure_ms":1587.7450000000001,"kind":"closure","repetition":4,"process_ms":2310.000,"peak_rss_bytes":150552576}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.30100000000004457,"closure_ms":893.5869999999999,"kind":"closure","repetition":5,"process_ms":1310.000,"peak_rss_bytes":152567808}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.6319999999999482,"closure_ms":4660.9310000000005,"kind":"closure","repetition":1,"process_ms":5350.000,"peak_rss_bytes":152993792}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.6409999999999627,"closure_ms":4526.305,"kind":"closure","repetition":2,"process_ms":4980.000,"peak_rss_bytes":149307392}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.26099999999996726,"closure_ms":3493.496,"kind":"closure","repetition":3,"process_ms":3820.000,"peak_rss_bytes":152535040}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.5690000000000168,"closure_ms":2772.203,"kind":"closure","repetition":4,"process_ms":3240.000,"peak_rss_bytes":146112512}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.23799999999999955,"closure_ms":2356.972,"kind":"closure","repetition":5,"process_ms":2640.000,"peak_rss_bytes":152502272}
```
