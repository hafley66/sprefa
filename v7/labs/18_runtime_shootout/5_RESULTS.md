# Native-logic runtime shootout results

- Generated: 2026-09-03 23:52:50 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 24 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| dbsp-kernel | 0.1.0 | 0.000 | 2162688 |
| racket | 9.3 | 220.000 | 161562624 |
| sbcl | 2.6.7 | 10.000 | 41205760 |
| swi | 10.0.2 | 0.000 | 7372800 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dbsp-kernel | chain | 47 | 1128 | 0.015875 | 4.087625 | 0.000 | 2490368 |
| dbsp-kernel | ring | 48 | 2304 | 0.019041 | 6.986708 | 0.000 | 3489792 |
| racket | chain | 47 | 1128 | 0.2259999999999991 | 379.509 | 600.000 | 152600576 |
| racket | ring | 48 | 2304 | 0.23099999999999454 | 1581.291 | 1790.000 | 152666112 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.123 | 20.000 | 46399488 |
| sbcl | ring | 48 | 2304 | 0.001 | 0.248 | 20.000 | 46661632 |
| swi | chain | 47 | 1128 | 0 | 1 | 10.000 | 10420224 |
| swi | ring | 48 | 2304 | 0 | 2 | 20.000 | 10895360 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":10.000,"peak_rss_bytes":41140224}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":10.000,"peak_rss_bytes":40992768}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":10.000,"peak_rss_bytes":41205760}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":10.000,"peak_rss_bytes":41041920}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":10.000,"peak_rss_bytes":41107456}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":7372800}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":7258112}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":7028736}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":7372800}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":220.000,"peak_rss_bytes":161562624}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":220.000,"peak_rss_bytes":161251328}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":220.000,"peak_rss_bytes":161366016}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":220.000,"peak_rss_bytes":161366016}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":220.000,"peak_rss_bytes":161218560}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1998848}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":2162688}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1966080}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1966080}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1966080}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.002,"closure_ms":0.120,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46137344}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46350336}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.122,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46399488}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46284800}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.123,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46301184}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.246,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46661632}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.252,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46333952}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.248,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46661632}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.248,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46415872}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.264,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46579712}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":10420224}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":2,"process_ms":10.000,"peak_rss_bytes":10305536}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":9928704}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":10059776}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":9961472}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":10272768}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":10584064}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":10321920}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":1,"closure_ms":2,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":10256384}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":10895360}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2259999999999991,"closure_ms":389.44100000000003,"kind":"closure","repetition":1,"process_ms":600.000,"peak_rss_bytes":152420352}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22799999999998022,"closure_ms":374.04300000000006,"kind":"closure","repetition":2,"process_ms":590.000,"peak_rss_bytes":152354816}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22399999999998954,"closure_ms":375.603,"kind":"closure","repetition":3,"process_ms":600.000,"peak_rss_bytes":152289280}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2330000000000041,"closure_ms":379.509,"kind":"closure","repetition":4,"process_ms":590.000,"peak_rss_bytes":152469504}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2259999999999991,"closure_ms":515.329,"kind":"closure","repetition":5,"process_ms":730.000,"peak_rss_bytes":152600576}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.41399999999998727,"closure_ms":3644.657,"kind":"closure","repetition":1,"process_ms":4200.000,"peak_rss_bytes":150339584}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.2829999999999586,"closure_ms":2579.531,"kind":"closure","repetition":2,"process_ms":2910.000,"peak_rss_bytes":152666112}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.21999999999999886,"closure_ms":1581.291,"kind":"closure","repetition":3,"process_ms":1790.000,"peak_rss_bytes":152420352}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.23099999999999454,"closure_ms":1311.525,"kind":"closure","repetition":4,"process_ms":1520.000,"peak_rss_bytes":152256512}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.2259999999999991,"closure_ms":1320.362,"kind":"closure","repetition":5,"process_ms":1530.000,"peak_rss_bytes":152190976}
{"case":"chain","closure_count":1128,"closure_ms":4.099625,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015875,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2490368}
{"case":"chain","closure_count":1128,"closure_ms":4.064583,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.018084000000000003,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2490368}
{"case":"chain","closure_count":1128,"closure_ms":4.087625,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.016375,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2490368}
{"case":"chain","closure_count":1128,"closure_ms":4.059041000000001,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015791,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2490368}
{"case":"chain","closure_count":1128,"closure_ms":4.183,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015875,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2490368}
{"case":"ring","closure_count":2304,"closure_ms":6.995834,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.019041,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":3276800}
{"case":"ring","closure_count":2304,"closure_ms":6.952833,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.017499999999999998,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":3293184}
{"case":"ring","closure_count":2304,"closure_ms":6.957458,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.032416,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":3031040}
{"case":"ring","closure_count":2304,"closure_ms":6.986708,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.020791,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":3031040}
{"case":"ring","closure_count":2304,"closure_ms":7.212750000000001,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.016208,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":3489792}
```
