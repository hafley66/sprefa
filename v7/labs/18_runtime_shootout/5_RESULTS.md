# Native-logic runtime shootout results

- Generated: 2026-09-04 00:35:12 EDT
- Machine: arm64, 14.6.1
- N: 48
- Protocol: one warmup, five measured repetitions
- Total measured harness wall time: 16 seconds

Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.

## Process startup

| Runtime | Version | Median startup ms | Peak RSS bytes |
| --- | --- | ---: | ---: |
| dbsp-generated | 0.1.0 | 0.000 | 1916928 |
| dbsp-kernel | 0.1.0 | 0.000 | 1916928 |
| racket | 9.3 | 210.000 | 161513472 |
| sbcl | 2.6.7 | 10.000 | 41189376 |
| swi | 10.0.2 | 0.000 | 7405568 |

## Closure cases

| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| dbsp-generated | chain | 47 | 1128 | 0.020999999999999998 | 4.078458 | 0.000 | 2621440 |
| dbsp-generated | ring | 48 | 2304 | 0.024042 | 6.935125 | 0.000 | 3588096 |
| dbsp-kernel | chain | 47 | 1128 | 0.015458999999999999 | 4.113208 | 0.000 | 2768896 |
| dbsp-kernel | ring | 48 | 2304 | 0.016791 | 6.974292 | 0.000 | 3424256 |
| racket | chain | 47 | 1128 | 0.22399999999998954 | 368.023 | 570.000 | 152584192 |
| racket | ring | 48 | 2304 | 0.23199999999999932 | 1319.431 | 1530.000 | 152698880 |
| sbcl | chain | 47 | 1128 | 0.001 | 0.118 | 20.000 | 46432256 |
| sbcl | ring | 48 | 2304 | 0.001 | 0.255 | 20.000 | 46546944 |
| swi | chain | 47 | 1128 | 0 | 1 | 10.000 | 10190848 |
| swi | ring | 48 | 2304 | 0 | 2 | 20.000 | 10616832 |

## Measured records

```jsonl
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":1,"process_ms":10.000,"peak_rss_bytes":40976384}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":2,"process_ms":10.000,"peak_rss_bytes":41189376}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":3,"process_ms":10.000,"peak_rss_bytes":41041920}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":4,"process_ms":10.000,"peak_rss_bytes":41074688}
{"kind":"startup","runtime":"sbcl","version":"2.6.7","repetition":5,"process_ms":10.000,"peak_rss_bytes":41189376}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":1,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":2,"process_ms":0.000,"peak_rss_bytes":7405568}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":3,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":4,"process_ms":0.000,"peak_rss_bytes":7356416}
{"kind":"startup","runtime":"swi","version":"10.0.2","repetition":5,"process_ms":0.000,"peak_rss_bytes":7127040}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":1,"process_ms":210.000,"peak_rss_bytes":161267712}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":2,"process_ms":220.000,"peak_rss_bytes":161415168}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":3,"process_ms":210.000,"peak_rss_bytes":161316864}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":4,"process_ms":210.000,"peak_rss_bytes":161513472}
{"kind":"startup","runtime":"racket","version":"9.3","repetition":5,"process_ms":210.000,"peak_rss_bytes":161202176}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-kernel","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":1,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":2,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":3,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":4,"process_ms":0.000,"peak_rss_bytes":1916928}
{"kind":"startup","runtime":"dbsp-generated","version":"0.1.0","repetition":5,"process_ms":0.000,"peak_rss_bytes":1916928}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.117,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46268416}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.118,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46186496}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.117,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46383104}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.122,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46432256}
{"runtime":"sbcl","version":"2.6.7","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.001,"closure_ms":0.125,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46153728}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.264,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":46514176}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.257,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":46530560}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.251,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":46530560}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.255,"kind":"closure","repetition":4,"process_ms":20.000,"peak_rss_bytes":46465024}
{"runtime":"sbcl","version":"2.6.7","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.001,"closure_ms":0.247,"kind":"closure","repetition":5,"process_ms":20.000,"peak_rss_bytes":46546944}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":1,"process_ms":10.000,"peak_rss_bytes":10125312}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":2,"process_ms":10.000,"peak_rss_bytes":9846784}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":3,"process_ms":10.000,"peak_rss_bytes":9912320}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":10190848}
{"runtime":"swi","version":"10.0.2","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0,"closure_ms":1,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":9797632}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":1,"process_ms":20.000,"peak_rss_bytes":10616832}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":3,"kind":"closure","repetition":2,"process_ms":20.000,"peak_rss_bytes":10436608}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":3,"process_ms":20.000,"peak_rss_bytes":10436608}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":4,"process_ms":10.000,"peak_rss_bytes":10190848}
{"runtime":"swi","version":"10.0.2","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0,"closure_ms":2,"kind":"closure","repetition":5,"process_ms":10.000,"peak_rss_bytes":10158080}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.23400000000000887,"closure_ms":367.44,"kind":"closure","repetition":1,"process_ms":570.000,"peak_rss_bytes":150339584}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.21999999999999886,"closure_ms":365.945,"kind":"closure","repetition":2,"process_ms":570.000,"peak_rss_bytes":152207360}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22399999999998954,"closure_ms":368.023,"kind":"closure","repetition":3,"process_ms":570.000,"peak_rss_bytes":150192128}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.2230000000000132,"closure_ms":373.076,"kind":"closure","repetition":4,"process_ms":580.000,"peak_rss_bytes":152584192}
{"runtime":"racket","version":"9.3","case":"chain","n":48,"edge_count":47,"closure_count":1128,"setup_ms":0.22800000000000864,"closure_ms":370.56899999999996,"kind":"closure","repetition":5,"process_ms":570.000,"peak_rss_bytes":152387584}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.23199999999999932,"closure_ms":1319.431,"kind":"closure","repetition":1,"process_ms":1530.000,"peak_rss_bytes":152698880}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22999999999998977,"closure_ms":1329.56,"kind":"closure","repetition":2,"process_ms":1540.000,"peak_rss_bytes":152420352}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.24399999999999977,"closure_ms":1312.808,"kind":"closure","repetition":3,"process_ms":1510.000,"peak_rss_bytes":152207360}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.23199999999999932,"closure_ms":1333.391,"kind":"closure","repetition":4,"process_ms":1530.000,"peak_rss_bytes":152272896}
{"runtime":"racket","version":"9.3","case":"ring","n":48,"edge_count":48,"closure_count":2304,"setup_ms":0.22100000000000364,"closure_ms":1316.853,"kind":"closure","repetition":5,"process_ms":1520.000,"peak_rss_bytes":152666112}
{"case":"chain","closure_count":1128,"closure_ms":4.041708,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015458999999999999,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2703360}
{"case":"chain","closure_count":1128,"closure_ms":4.0346660000000005,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015917,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2719744}
{"case":"chain","closure_count":1128,"closure_ms":4.113208,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.01475,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2441216}
{"case":"chain","closure_count":1128,"closure_ms":4.1224169999999996,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.014875000000000001,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2768896}
{"case":"chain","closure_count":1128,"closure_ms":4.211291,"edge_count":47,"n":48,"runtime":"dbsp-kernel","setup_ms":0.0155,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2654208}
{"case":"ring","closure_count":2304,"closure_ms":7.035708,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015083,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":7.020708,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.018667,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":3244032}
{"case":"ring","closure_count":2304,"closure_ms":6.974292,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.020417,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":3424256}
{"case":"ring","closure_count":2304,"closure_ms":6.77175,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.016791,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":6.7795,"edge_count":48,"n":48,"runtime":"dbsp-kernel","setup_ms":0.015333000000000001,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"chain","closure_count":1128,"closure_ms":4.184625,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.020542,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2621440}
{"case":"chain","closure_count":1128,"closure_ms":4.078458,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.022500000000000003,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2441216}
{"case":"chain","closure_count":1128,"closure_ms":4.05225,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.025959,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2441216}
{"case":"chain","closure_count":1128,"closure_ms":4.095709,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.020958,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":2441216}
{"case":"chain","closure_count":1128,"closure_ms":3.9490000000000003,"edge_count":47,"n":48,"runtime":"dbsp-generated","setup_ms":0.020999999999999998,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":2441216}
{"case":"ring","closure_count":2304,"closure_ms":6.817374999999999,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.024042,"version":"0.1.0","kind":"closure","repetition":1,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":6.978541000000001,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.025625,"version":"0.1.0","kind":"closure","repetition":2,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":6.92775,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.022041,"version":"0.1.0","kind":"closure","repetition":3,"process_ms":0.000,"peak_rss_bytes":2981888}
{"case":"ring","closure_count":2304,"closure_ms":6.935125,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.022666,"version":"0.1.0","kind":"closure","repetition":4,"process_ms":0.000,"peak_rss_bytes":3489792}
{"case":"ring","closure_count":2304,"closure_ms":7.102125,"edge_count":48,"n":48,"runtime":"dbsp-generated","setup_ms":0.029,"version":"0.1.0","kind":"closure","repetition":5,"process_ms":0.000,"peak_rss_bytes":3588096}
```
