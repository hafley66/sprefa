(defpackage #:dl7-si-kanren-lab
  (:use #:cl)
  (:import-from #:si-kanren
                #:run
                #:fresh
                #:conde
                #:==
                #:=/=
                #:symbolo
                #:numbero
                #:absento)
  (:export #:main #:run-probe))

(in-package #:dl7-si-kanren-lab)

;; The fixture relation definitions use only exported si-kanren forms.
(defun edgeo (from to)
  (conde
    ((== `(a . b) `(,from . ,to)))
    ((== `(b . c) `(,from . ,to)))
    ((== `(c . a) `(,from . ,to)))
    ((== `(c . d) `(,from . ,to)))))

;; This adapter is a finite union of paths with one through four edges.  It is
;; deliberately non-recursive: maximum depth 4 plus RUN's 16-answer limit is
;; the hard bound for the cyclic fixture.
(defun path-at-most-4o (from to)
  (conde
    ((edgeo from to))
    ((fresh (a)
       (edgeo from a)
       (edgeo a to)))
    ((fresh (a b)
       (edgeo from a)
       (edgeo a b)
       (edgeo b to)))
    ((fresh (a b c)
       (edgeo from a)
       (edgeo a b)
       (edgeo b c)
       (edgeo c to)))))

;; The second branch is recursive.  RUN's answer bound is the hard finite
;; limit for this fairness probe.
(defun fives-forever-o (value)
  (conde
    ((== value 'five))
    ((fives-forever-o value))))

(defun query-output (thunk)
  (with-output-to-string (stream)
    (let ((*standard-output* stream))
      (funcall thunk))))

(defun record (name thunk)
  (format t "~A ~A~%" name (query-output thunk)))

(defun run-probe ()
  (format t "PROBE library=si-kanren quicklisp=2026-01-01 upstream=93f051fcc2b46649d214eab951cdd4ed1de869da~%")
  (record "UNIFY"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (== x '(alpha beta))
                (== q `(node ,x))))))
  (record "SUBSTITUTION"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (== x 'alpha)
                (== q `(,x (node ,x)))))))
  (record "OCCURS"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (== x `(f . ,x))
                (== q x)))))
  (record "ORDER"
          (lambda ()
            (run 4 (q)
              (conde
                ((== q 'left))
                ((== q 'right))
                ((== q 'center))))))
  (record "DUPLICATES"
          (lambda ()
            (run 4 (q)
              (conde
                ((== q 'duplicate))
                ((== q 'duplicate))))))
  (record "FAIRNESS_LIMIT_4"
          (lambda ()
            (run 4 (q)
              (conde
                ((fives-forever-o q))
                ((== q 'right))))))
  (record "PATH_DEPTH_4_LIMIT_16"
          (lambda ()
            (run 16 (q)
              (path-at-most-4o 'a q))))
  (record "DISEQUALITY_RESIDUAL"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (=/= x 'blocked)
                (== q x)))))
  (record "DISEQUALITY_VIOLATION"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (=/= x 'blocked)
                (== x 'blocked)
                (== q x)))))
  (record "NUMBERO_RESIDUAL"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (numbero x)
                (== q x)))))
  (record "NUMBERO_VIOLATION"
          (lambda ()
            (run 1 (q)
              (numbero 'not-a-number)
              (== q 'reachable))))
  (record "FIXTURE_SYMBOL_A"
          (lambda ()
            (run 1 (q)
              (symbolo 'a)
              (== q 'a))))
  (record "FIXTURE_NUMBER_A_REJECT"
          (lambda ()
            (run 1 (q)
              (numbero 'a)
              (== q 'reachable))))
  (record "SYMBOLO_RESIDUAL"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (symbolo x)
                (== q x)))))
  (record "SYMBOLO_VIOLATION"
          (lambda ()
            (run 1 (q)
              (symbolo 7)
              (== q 'reachable))))
  (record "ABSENTO_RESIDUAL"
          (lambda ()
            (run 1 (q)
              (fresh (x)
                (absento 'forbidden x)
                (== q x)))))
  (record "ABSENTO_VIOLATION"
          (lambda ()
            (run 1 (q)
              (absento 'forbidden '(allowed forbidden tail))
              (== q 'reachable))))
  (record "FIXTURE_ABSENTO_D_FROM_A"
          (lambda ()
            (run 1 (q)
              (absento 'd 'a)
              (== q 'a))))
  (format t "CONSTRAINT_STORE (((s) . c) (d) (t) (a))~%"))

(defun main ()
  (handler-case
      (progn
        (run-probe)
        (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))

(run-probe)
