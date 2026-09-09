#lang racket/base

(require racklog
         racket/cmdline
         racket/list)

(define mode #f)
(define size #f)

(command-line
 #:program "racklog-benchmark"
 #:args (requested-mode requested-size)
 (set! mode requested-mode)
 (set! size (string->number requested-size)))

(unless (and (exact-positive-integer? size)
             (member mode '("assert" "chain" "cycle")))
  (raise-user-error
   'racklog-benchmark
   "usage: racket 3_RACKLOG.rkt <assert|chain|cycle> <positive-size>"))

(define (elapsed thunk)
  (collect-garbage)
  (define start (current-inexact-monotonic-milliseconds))
  (define result (thunk))
  (values result (- (current-inexact-monotonic-milliseconds) start)))

(define %edge %empty-rel)

(define %path
  (%rel (x y z)
    [(x y) (%edge x y)]
    [(x y) (%edge x z) (%path z y)]))

(define (assert-chain! count cycle?)
  (set! %edge %empty-rel)
  (for ([source (in-range count)])
    (define target
      (if (and cycle? (= source (sub1 count)))
          0
          (add1 source)))
    (%assert-after! %edge ()
      [(source target)])))

(define (emit mode count assertion-ms query-ms answers)
  (printf
   "engine=racklog mode=~a n=~a assert_ms=~a query_ms=~a answers=~a~n"
   mode count assertion-ms query-ms answers))

(case mode
  [("assert")
   (define-values (_ assertion-ms)
     (elapsed (lambda () (assert-chain! size #f))))
   (emit mode size assertion-ms 0 size)]
  [("chain")
   (define-values (_ assertion-ms)
     (elapsed (lambda () (assert-chain! size #f))))
   (define-values (answers query-ms)
     (elapsed
      (lambda ()
        (%find-all (y) (%path 0 y)))))
   (emit mode size assertion-ms query-ms (length answers))]
  [("cycle")
   (define-values (_ assertion-ms)
     (elapsed (lambda () (assert-chain! size #t))))
   (define result-channel (make-channel))
   (define worker
     (thread
      (lambda ()
        (channel-put
         result-channel
         (%find-all (y) (%path 0 y))))))
   (define start (current-inexact-monotonic-milliseconds))
   (define answers (sync/timeout 1.0 result-channel))
   (define query-ms (- (current-inexact-monotonic-milliseconds) start))
   (unless answers
     (kill-thread worker))
   (emit mode size assertion-ms query-ms
         (if answers (length answers) "timeout"))])

