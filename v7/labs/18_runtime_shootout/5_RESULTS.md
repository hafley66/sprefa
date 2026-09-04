# Native-logic runtime shootout results

- Generated: 2026-09-04 00:56:59 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 17 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| dbsp-generated | 0.1.0 | 0.000 | 1949696 |
| dbsp-kernel | 0.1.0 | 0.000 | 2179072 |
| dbsp-sqlite | 3.53.2 | 0.000 | 3129344 |
| racket | 9.3 | 220.000 | 161677312 |
| sbcl | 2.6.7 | 10.000 | 41222144 |
| swi | 10.0.2 | 0.000 | 8011776 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dbsp-generated | chain | 47 | 1128 | 0.021375 | 4.109708 | 0.000 | 2932736 |
| dbsp-generated | ring | 48 | 2304 | 0.019416 | 6.940334 | 0.000 | 3801088 |
| dbsp-kernel | chain | 47 | 1128 | 0.015083 | 4.0545 | 0.000 | 2473984 |
| dbsp-kernel | ring | 48 | 2304 | 0.014 | 6.940375 | 0.000 | 3899392 |
| dbsp-sqlite | chain | 47 | 1128 | 0.236792 | 33.596958 | 30.000 | 5521408 |
| dbsp-sqlite | ring | 48 | 2304 | 0.23541600000000001 | 59.488916 | 60.000 | 7634944 |
| racket | chain | 47 | 1128 | 0.22100000000000364 | 368.64199999999994 | 570.000 | 152551424 |
| racket | ring | 48 | 2304 | 0.23799999999999955 | 1321.179 | 1530.000 | 152502272 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.126 | 20.000 | 46481408 |
| sbcl | ring | 48 | 2304 | 0.001 | 0.246 | 20.000 | 46514176 |
| swi | chain | 47 | 1128 | 0 | 1 | 10.000 | 10256384 |
| swi | ring | 48 | 2304 | 0 | 3 | 10.000 | 10305536 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":10.000,"peak_rss_bytes":41025536}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":10.000,"peak_rss_bytes":41222144}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":10.000,"peak_rss_bytes":41123840}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":10.000,"peak_rss_bytes":40976384}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":10.000,"peak_rss_bytes":41189376}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":7127040}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":7127040}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":8011776}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":7651328}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":210.000,"peak_rss_bytes":161464320}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":220.000,"peak_rss_bytes":161120256}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":220.000,"peak_rss_bytes":160972800}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":220.000,"peak_rss_bytes":161366016}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":210.000,"peak_rss_bytes":161677312}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":2179072}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1900544}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1900544}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1900544}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1900544}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1949696}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1949696}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":3080192}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":3080192}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":3129344}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":3080192}
{"kind":"startup","runtime":"dbsp-sqlite","version":"3.53.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":3080192}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.124,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46399488}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.127,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46399488}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46088192}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.126,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46481408}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.123,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.258,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.257,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46497792}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.245,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46514176}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.246,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46481408}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.236,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46514176}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":10.000,"peak_rss_bytes":9535488}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":2,"process_ms":10.000,"peak_rss_bytes":9699328}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":10125312}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":9912320}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":1,"closure_ms":1,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":10256384}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":1,"process_ms":10.000,"peak_rss_bytes":9830400}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":2,"process_ms":10.000,"peak_rss_bytes":10272768}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":10305536}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":10092544}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":1,"closure_ms":2,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":10190848}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22499999999999432,"closure_ms":370.1,"kind":"closure","repetition":1,"process_ms":580.000,"peak_rss_bytes":152551424}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2189999999999941,"closure_ms":366.686,"kind":"closure","repetition":2,"process_ms":570.000,"peak_rss_bytes":152485888}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22100000000000364,"closure_ms":372.613,"kind":"closure","repetition":3,"process_ms":580.000,"peak_rss_bytes":152272896}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22999999999998977,"closure_ms":368.64199999999994,"kind":"closure","repetition":4,"process_ms":570.000,"peak_rss_bytes":152240128}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2189999999999941,"closure_ms":367.34700000000004,"kind":"closure","repetition":5,"process_ms":570.000,"peak_rss_bytes":152289280}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.23799999999999955,"closure_ms":1321.179,"kind":"closure","repetition":1,"process_ms":1530.000,"peak_rss_bytes":152502272}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22499999999999432,"closure_ms":1333.679,"kind":"closure","repetition":2,"process_ms":1540.000,"peak_rss_bytes":152305664}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.24499999999997613,"closure_ms":1332.873,"kind":"closure","repetition":3,"process_ms":1540.000,"peak_rss_bytes":152240128}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.25899999999998613,"closure_ms":1320.229,"kind":"closure","repetition":4,"process_ms":1530.000,"peak_rss_bytes":150503424}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.228999999999985,"closure_ms":1311.999,"kind":"closure","repetition":5,"process_ms":1520.000,"peak_rss_bytes":152256512}
{"case":"chain","closure_count":1128,"closure_ms":4.07775,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.020333,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2424832}
{"case":"chain","closure_count":1128,"closure_ms":3.9707500000000002,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015083,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2424832}
{"case":"chain","closure_count":1128,"closure_ms":4.086417,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.012875,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2473984}
{"case":"chain","closure_count":1128,"closure_ms":3.9481669999999998,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.01525,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2473984}
{"case":"chain","closure_count":1128,"closure_ms":4.0545,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.013042,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2424832}
{"case":"ring","closure_count":2304,"closure_ms":6.95175,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.012,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"ring","closure_count":2304,"closure_ms":6.940375,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.013083,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":3899392}
{"case":"ring","closure_count":2304,"closure_ms":6.980833,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":3309568}
{"case":"ring","closure_count":2304,"closure_ms":6.878,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014208,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"ring","closure_count":2304,"closure_ms":6.810625,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014875000000000001,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"chain","closure_count":1128,"closure_ms":4.101167,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.017542,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2736128}
{"case":"chain","closure_count":1128,"closure_ms":4.172834,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.019875,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2932736}
{"case":"chain","closure_count":1128,"closure_ms":4.109708,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.021792000000000002,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2424832}
{"case":"chain","closure_count":1128,"closure_ms":4.098667000000001,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.022792,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2654208}
{"case":"chain","closure_count":1128,"closure_ms":4.126666,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.021375,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2736128}
{"case":"ring","closure_count":2304,"closure_ms":6.977958999999999,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.017875000000000002,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"ring","closure_count":2304,"closure_ms":6.940334,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019416,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"ring","closure_count":2304,"closure_ms":6.950457999999999,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019167,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":3276800}
{"case":"ring","closure_count":2304,"closure_ms":6.804459,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.019541000000000003,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":3801088}
{"case":"ring","closure_count":2304,"closure_ms":6.731625,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.025166,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2965504}
{"case":"chain","closure_count":1128,"closure_ms":33.819458,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.236792,"version":"3.53.2","kind":"closure","repetition":1,"process_ms":30.000,"peak_rss_bytes":5455872}
{"case":"chain","closure_count":1128,"closure_ms":33.596958,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.267375,"version":"3.53.2","kind":"closure","repetition":2,"process_ms":30.000,"peak_rss_bytes":5505024}
{"case":"chain","closure_count":1128,"closure_ms":33.414917,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.23575000000000002,"version":"3.53.2","kind":"closure","repetition":3,"process_ms":30.000,"peak_rss_bytes":5521408}
{"case":"chain","closure_count":1128,"closure_ms":33.503708,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.221417,"version":"3.53.2","kind":"closure","repetition":4,"process_ms":30.000,"peak_rss_bytes":5193728}
{"case":"chain","closure_count":1128,"closure_ms":34.08675,"edge_count":47,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.29500000000000004,"version":"3.53.2","kind":"closure","repetition":5,"process_ms":30.000,"peak_rss_bytes":5029888}
{"case":"ring","closure_count":2304,"closure_ms":59.459792,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.22479100000000002,"version":"3.53.2","kind":"closure","repetition":1,"process_ms":60.000,"peak_rss_bytes":6914048}
{"case":"ring","closure_count":2304,"closure_ms":59.488916,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.23541600000000001,"version":"3.53.2","kind":"closure","repetition":2,"process_ms":60.000,"peak_rss_bytes":6340608}
{"case":"ring","closure_count":2304,"closure_ms":59.755333,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.229125,"version":"3.53.2","kind":"closure","repetition":3,"process_ms":60.000,"peak_rss_bytes":7634944}
{"case":"ring","closure_count":2304,"closure_ms":59.905917,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.271583,"version":"3.53.2","kind":"closure","repetition":4,"process_ms":60.000,"peak_rss_bytes":7045120}
{"case":"ring","closure_count":2304,"closure_ms":59.071791999999995,"edge_count":48,"n":48,"runtime":"dbsp-sqlite","setup_ms":0.26491699999999996,"version":"3.53.2","kind":"closure","repetition":5,"process_ms":60.000,"peak_rss_bytes":6799360}
```
