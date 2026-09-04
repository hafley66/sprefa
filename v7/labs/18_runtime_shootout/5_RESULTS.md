# Native-logic runtime shootout results

- Generated: 2026-09-04 01:09:00 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 19 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| dbsp-generated | 0.1.0 | 0.000 | 1884160 |
| dbsp-kernel | 0.1.0 | 0.000 | 1884160 |
| dbsp-sqlite | 3.53.2 | 0.000 | 3145728 |
| racket | 9.3 | 210.000 | 161660928 |
| sbcl | 2.6.7 | 10.000 | 41140224 |
| swi | 10.0.2 | 0.000 | 7503872 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dbsp-generated | chain | 47 | 1128 | 0.024042 | 4.060666 | 0.000 | 2605056 |
| dbsp-generated | ring | 48 | 2304 | 0.019875 | 6.935459 | 0.000 | 3964928 |
| dbsp-kernel | chain | 47 | 1128 | 0.014957999999999999 | 3.9905 | 0.000 | 2408448 |
| dbsp-kernel | ring | 48 | 2304 | 0.016583 | 6.94275 | 0.000 | 3457024 |
| dbsp-sqlite | chain | 47 | 1128 | 0.309083 | 33.761333 | 30.000 | 5652480 |
| dbsp-sqlite | ring | 48 | 2304 | 0.313584 | 59.989166 | 60.000 | 8339456 |
| racket | chain | 47 | 1128 | 0.2230000000000132 | 367.095 | 570.000 | 152354816 |
| racket | ring | 48 | 2304 | 0.22700000000000387 | 1313.6589999999999 | 1520.000 | 152420352 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.120 | 20.000 | 46530560 |
| sbcl | ring | 48 | 2304 | 0.001 | 0.241 | 20.000 | 46596096 |
| swi | chain | 47 | 1128 | 0 | 1 | 10.000 | 11288576 |
| swi | ring | 48 | 2304 | 0 | 3 | 10.000 | 10911744 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":10.000,"peak_rss_bytes":41140224}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":10.000,"peak_rss_bytes":41074688}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":10.000,"peak_rss_bytes":41041920}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":10.000,"peak_rss_bytes":41009152}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":10.000,"peak_rss_bytes":41041920}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":7503872}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":7503872}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":7454720}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":7405568}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":220.000,"peak_rss_bytes":161153024}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":220.000,"peak_rss_bytes":161579008}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":210.000,"peak_rss_bytes":161366016}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":210.000,"peak_rss_bytes":161660928}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":210.000,"peak_rss_bytes":161464320}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1884160}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":3145728}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":3145728}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":3145728}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":3145728}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":3145728}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.116,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46530560}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.113,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.120,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46514176}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.121,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46317568}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.002,"closure_ms":0.120,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46284800}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.241,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46333952}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.239,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46252032}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.000,"closure_ms":0.232,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46514176}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.255,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46333952}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.254,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46596096}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":10.000,"peak_rss_bytes":9879552}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":11288576}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":10190848}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":9912320}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":10125312}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":1,"process_ms":10.000,"peak_rss_bytes":10256384}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":1,"closure_ms":2,"kind":"closure","repetition":2,"process_ms":10.000,"peak_rss_bytes":10911744}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":10223616}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":10174464}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":10387456}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2230000000000132,"closure_ms":367.0949999999999,"kind":"closure","repetition":1,"process_ms":570.000,"peak_rss_bytes":152354816}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.21699999999998454,"closure_ms":369.49199999999996,"kind":"closure","repetition":2,"process_ms":570.000,"peak_rss_bytes":150339584}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22800000000000864,"closure_ms":366.83799999999997,"kind":"closure","repetition":3,"process_ms":570.000,"peak_rss_bytes":152289280}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22700000000000387,"closure_ms":368.15000000000003,"kind":"closure","repetition":4,"process_ms":570.000,"peak_rss_bytes":152289280}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.21999999999999886,"closure_ms":367.095,"kind":"closure","repetition":5,"process_ms":570.000,"peak_rss_bytes":152207360}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22700000000000387,"closure_ms":1313.6589999999999,"kind":"closure","repetition":1,"process_ms":1520.000,"peak_rss_bytes":152305664}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22700000000000387,"closure_ms":1319.069,"kind":"closure","repetition":2,"process_ms":1530.000,"peak_rss_bytes":150437888}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22700000000000387,"closure_ms":1308.991,"kind":"closure","repetition":3,"process_ms":1510.000,"peak_rss_bytes":152420352}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.21600000000000819,"closure_ms":1316.62,"kind":"closure","repetition":4,"process_ms":1520.000,"peak_rss_bytes":152207360}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.2220000000000084,"closure_ms":1309.2600000000002,"kind":"closure","repetition":5,"process_ms":1510.000,"peak_rss_bytes":152256512}
{"case":"chain","closure_count":1128,"closure_ms":3.9905,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015125,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":4.0315840000000005,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015541,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":3.972833,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014957999999999999,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":4.004542,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":3.9695420000000006,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014333,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"ring","closure_count":2304,"closure_ms":7.04475,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.013417,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":3457024}
{"case":"ring","closure_count":2304,"closure_ms":7.141625,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":3276800}
{"case":"ring","closure_count":2304,"closure_ms":6.767417,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.018792,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2949120}
{"case":"ring","closure_count":2304,"closure_ms":6.94275,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.017291,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2949120}
{"case":"ring","closure_count":2304,"closure_ms":6.924625,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.016583,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2949120}
{"case":"chain","closure_count":1128,"closure_ms":4.0367500000000005,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.026667,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":4.0988750000000005,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.024042,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":4.120875000000001,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.026375,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2605056}
{"case":"chain","closure_count":1128,"closure_ms":4.0555829999999995,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.022500000000000003,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"chain","closure_count":1128,"closure_ms":4.060666,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.020665999999999997,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2408448}
{"case":"ring","closure_count":2304,"closure_ms":6.935459,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.020999999999999998,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2949120}
{"case":"ring","closure_count":2304,"closure_ms":6.881916,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.021083,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2949120}
{"case":"ring","closure_count":2304,"closure_ms":6.780584,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019125,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":7.093875000000001,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019083,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":3964928}
{"case":"ring","closure_count":2304,"closure_ms":6.972709,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019875,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":3309568}
{"case":"chain","closure_count":1128,"closure_ms":33.647000000000006,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.293875,"version":"3.53.2","kind":"closure","repetition":1,"process_ms":30.000,"peak_rss_bytes":5652480}
{"case":"chain","closure_count":1128,"closure_ms":34.345417000000005,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.309083,"version":"3.53.2","kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":5505024}
{"case":"chain","closure_count":1128,"closure_ms":33.523792,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.29924999999999996,"version":"3.53.2","kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":5160960}
{"case":"chain","closure_count":1128,"closure_ms":33.761333,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.312667,"version":"3.53.2","kind":"closure","repetition":4,"process_ms":30.000,"peak_rss_bytes":5505024}
{"case":"chain","closure_count":1128,"closure_ms":33.86625,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.361417,"version":"3.53.2","kind":"closure","repetition":5,"process_ms":30.000,"peak_rss_bytes":5079040}
{"case":"ring","closure_count":2304,"closure_ms":59.645916,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.28279200000000004,"version":"3.53.2","kind":"closure","repetition":1,"process_ms":60.000,"peak_rss_bytes":6979584}
{"case":"ring","closure_count":2304,"closure_ms":59.689333,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.304458,"version":"3.53.2","kind":"closure","repetition":2,"process_ms":60.000,"peak_rss_bytes":7176192}
{"case":"ring","closure_count":2304,"closure_ms":60.307583,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.32725,"version":"3.53.2","kind":"closure","repetition":3,"process_ms":60.000,"peak_rss_bytes":7143424}
{"case":"ring","closure_count":2304,"closure_ms":60.125333000000005,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.314875,"version":"3.53.2","kind":"closure","repetition":4,"process_ms":60.000,"peak_rss_bytes":8339456}
{"case":"ring","closure_count":2304,"closure_ms":59.989166,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.313584,"version":"3.53.2","kind":"closure","repetition":5,"process_ms":60.000,"peak_rss_bytes":6209536}
```
