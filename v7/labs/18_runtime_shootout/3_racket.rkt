#!/usr/bin/env racket
#lang racket/base

;; Racket arm for the runtime shootout. Uses the installed `datalog`
;; package's Datalog evaluator for recursive reachability. Native route:
;; edge facts asserted per graph edge, two recursive reach rules, one
;; query materializing the full (From, To) closure. Timing boundary:
;; setup_ms covers theory creation and edge-fact assertion;
;; closure_ms covers the datalog query and answer materialization.

(require racket/cmdline
         racket/set
         racket/string
         datalog)

(define expected-version (version))

;; Monotonic wall-clock milliseconds.
(define (now-ms)
  (current-inexact-monotonic-milliseconds))

(define (node-sym i)
  (string->symbol (string-append "n" (number->string i))))

(define (build-edges case n)
  (for/list ([i (in-range (if (string=? case "chain") (sub1 n) n))])
    (define j (modulo (add1 i) n))
    (list (node-sym i) (node-sym j))))

(define (run case n)
  (define edges (build-edges case n))
  (define edge-count (length edges))

  (define t0 (now-ms))
  (define theory (make-theory))
  (for ([e edges])
    (datalog theory
      (! (edge (car e) (cadr e)))))
  (datalog theory
    (! (:- (reach X Y) (edge X Y)))
    (! (:- (reach X Z) (edge X Y) (reach Y Z))))
  (define t1 (now-ms))

  (define answers
    (datalog theory
      (? (reach X Y))))
  (define pairs
    (for/set ([h (in-list answers)])
      (cons (hash-ref h 'X) (hash-ref h 'Y))))
  (define closure-count (set-count pairs))
  (define t2 (now-ms))

  (values edge-count closure-count (- t1 t0) (- t2 t1)))

(define (expect-count case n)
  (if (string=? case "chain")
      (quotient (* n (sub1 n)) 2)
      (* n n)))

(define (main)
  (define case-arg (make-parameter #f))
  (define n-arg (make-parameter #f))
  (command-line
   #:args (case n)
   (case-arg case)
   (n-arg n))
  (define case (case-arg))
  (define n (n-arg))
  (unless (and (string? case) (or (string=? case "chain") (string=? case "ring")))
    (error 'racket-arm "CASE must be 'chain' or 'ring', got: ~a" case))
  (define n-int (string->number n))
  (unless (and n-int (integer? n-int) (> n-int 0))
    (error 'racket-arm "N must be an integer > 0, got: ~a" n))

  (define-values (edge-count closure-count setup-ms closure-ms)
    (run case n-int))

  (define expected (expect-count case n-int))
  (unless (= closure-count expected)
    (error 'racket-arm "closure count mismatch: expected ~a, got ~a" expected closure-count))

  (printf "{\"runtime\":\"racket\",\"version\":\"~a\",\"case\":\"~a\",\"n\":~a,\"edge_count\":~a,\"closure_count\":~a,\"setup_ms\":~a,\"closure_ms\":~a}\n"
          expected-version case n-int edge-count closure-count setup-ms closure-ms))

(main)
