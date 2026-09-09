#lang racket/base

(require datalog
         racket/cmdline
         racket/list)

(define mode #f)
(define size #f)

(command-line
 #:program "racket-datalog-benchmark"
 #:args (requested-mode requested-size)
 (set! mode requested-mode)
 (set! size (string->number requested-size)))

(unless (and (exact-positive-integer? size)
             (member mode '("assert" "cycle" "all-pairs")))
  (raise-user-error
   'racket-datalog-benchmark
   "usage: racket 2_RACKET_DATALOG.rkt <assert|cycle|all-pairs> <positive-size>"))

(define (elapsed thunk)
  (collect-garbage)
  (define start (current-inexact-monotonic-milliseconds))
  (define result (thunk))
  (values result (- (current-inexact-monotonic-milliseconds) start)))

(define (elapsed/timeout seconds thunk)
  (collect-garbage)
  (define result-channel (make-channel))
  (define start (current-inexact-monotonic-milliseconds))
  (define worker
    (thread
     (lambda ()
       (channel-put result-channel (vector (thunk))))))
  (define wrapped-result (sync/timeout seconds result-channel))
  (define elapsed-ms (- (current-inexact-monotonic-milliseconds) start))
  (unless wrapped-result
    (kill-thread worker))
  (values
   (if wrapped-result (vector-ref wrapped-result 0) 'timeout)
   elapsed-ms))

(define (assert-chain! theory count cycle?)
  (for ([source (in-range count)])
    (define target
      (if (and cycle? (= source (sub1 count)))
          0
          (add1 source)))
    (datalog theory
             (! (edge #,source #,target)))))

(define (assert-path-rules! theory)
  (datalog theory
           (! (:- (path X Y)
                  (edge X Y)))
           (! (:- (path X Y)
                  (edge X Z)
                  (path Z Y)))))

(define (emit mode count assertion-ms query-ms answers)
  (printf
   "engine=racket-datalog mode=~a n=~a assert_ms=~a query_ms=~a answers=~a~n"
   mode count assertion-ms query-ms answers))

(case mode
  [("assert")
   (define theory (make-theory))
   (define-values (_ assertion-ms)
     (elapsed (lambda () (assert-chain! theory size #f))))
   (emit mode size assertion-ms 0 size)]
  [("cycle")
   (define theory (make-theory))
   (define-values (_ assertion-ms)
     (elapsed
      (lambda ()
        (assert-chain! theory size #t)
        (assert-path-rules! theory))))
   (define-values (answers query-ms)
     (elapsed/timeout
      10.0
      (lambda ()
        (datalog theory
                 (? (path #,0 Y)))))
   (emit mode size assertion-ms query-ms
         (if (eq? answers 'timeout) "timeout" (length answers)))]
  [("all-pairs")
   (define theory (make-theory))
   (define-values (_ assertion-ms)
     (elapsed
      (lambda ()
        (for ([source (in-range (sub1 size))])
          (datalog theory
                   (! (edge #,source #,(add1 source)))))
        (assert-path-rules! theory))))
   (define-values (answers query-ms)
     (elapsed/timeout
      10.0
      (lambda ()
        (datalog theory
                 (? (path X Y)))))
   (emit mode size assertion-ms query-ms
         (if (eq? answers 'timeout) "timeout" (length answers)))])
