;; file: 2a_FIXTURE.lisp
;; Fixture and probe sections for 2_PROBE.lisp. Loaded (not merely read)
;; after load-library, so cl-kanren symbols resolve at read time.
;; Divergent bodies carry their own in-process wall bounds.

(in-package #:cl-kanren-probe)

;; Finite fact store: a plain Lisp list consulted by membero. Updates are
;; setf on the store followed by re-query; the relational definitions read
;; the binding at goal-application time.
(defparameter *edges* '((a b) (b c) (c a) (c d)))
(defparameter *dedges* '((a b) (a c) (c b)))

(defun edgeo (x y)
  (cl-kanren:fresh (p)
    (cl-kanren:== p (list x y))
    (cl-kanren:membero p *edges*)))

(defun dedgeo (x y)
  (cl-kanren:fresh (p)
    (cl-kanren:== p (list x y))
    (cl-kanren:membero p *dedges*)))

(defun patho (x y)
  (cl-kanren:conde
    ((edgeo x y))
    ((cl-kanren:fresh (z)
       (edgeo x z)
       (patho z y)))))

(defun dpatho (x y)
  (cl-kanren:conde
    ((dedgeo x y))
    ((cl-kanren:fresh (z)
       (dedgeo x z)
       (dpatho z y)))))

(defun divergeo ()
  "A goal whose stream is an endless chain of thunks yielding nothing."
  (cl-kanren:fresh () (divergeo)))

(defun spino (x)
  "Infinite productive stream: one answer repeated forever."
  (cl-kanren:conde
    ((cl-kanren:== 'spin x))
    ((spino x))))

(defun not-edgeo (x y)
  "Bounded negation-as-failure adapter via ifte; safe only for ground pairs."
  (mu-kanren-goodies:ifte (edgeo x y)
                          cl-kanren:+fail+
                          cl-kanren:+succeed+))

(defparameter *sections* (make-hash-table :test #'equal))
(defmacro defsection (name &body body)
  `(setf (gethash ,name *sections*) (lambda () ,@body)))

(defsection "unify"
  ;; nested compound unification f(x, g(y)) = f(a, g(b)); both bindings
  ;; routed through q because reification reports only the first variable.
  (let ((r (cl-kanren:run 1 (q)
             (cl-kanren:fresh (x y)
               (cl-kanren:== (list 'f x (list 'g y)) '(f a (g b)))
               (cl-kanren:== (list x y) q)))))
    (format t "UNIFY ~s~%" r)))

(defsection "occurs"
  ;; exact occurs-check policy probe: X = f(X)
  (let ((r (cl-kanren:run 1 (x) (cl-kanren:== x (list 'f x)))))
    (format t "OCCURS occurs-check=present result=~a~%"
            (if (null r) "unification-fails" "unify-succeeds-cyclically"))))

(defsection "path"
  ;; cyclic transitive closure: infinite proof stream; termination only by
  ;; answer cap. Raw answers preserved; sorted set deduplicated.
  (let ((answers (cl-kanren:run 20 (x) (patho 'a x))))
    (format t "PATH raw=~a sorted=~a capped=t~%"
            answers (dedup (sorted-answers answers)))))

(defsection "path-unbounded"
  ;; run* over the same cycle diverges; the hard wall bound must fire.
  (let ((r (handler-case (sb-ext:with-timeout 5 (cl-kanren:run* (x) (patho 'a x)))
             (sb-ext:timeout () :timeout))))
    (format t "PATH-UNBOUNDED ~a~%"
            (if (eq r :timeout)
                "timeout:no-termination-without-cap"
                r))))

(defsection "dupes"
  ;; acyclic fixture with two proofs for B: duplicates remain in raw order.
  (let ((answers (cl-kanren:run 10 (x) (dpatho 'a x))))
    (format t "DUPES raw=~a sorted=~a~%" answers (sorted-answers answers))))

(defsection "order"
  ;; insertion order deliberately differs from lexical order.
  (let ((ordered (cl-kanren:run 10 (x)
                   (cl-kanren:conde
                     ((cl-kanren:== x 'z))
                     ((cl-kanren:== x 'a))))))
    (format t "ORDER raw=~a~%" ordered)))

(defsection "fair"
  ;; starvation: first clause diverges unproductively after the later fact
  ;; yields its single answer; requesting 3 answers hits the wall bound.
  (let ((r (handler-case
               (sb-ext:with-timeout 5
                 (cl-kanren:run 3 (x)
                   (cl-kanren:conde
                     ((divergeo))
                     ((cl-kanren:== 'done x)))))
             (sb-ext:timeout () :timeout))))
    (format t "FAIR-STARVE raw=~a~%"
            (if (eq r :timeout)
                "timeout:unproductive-first-branch-starves-after-first-answer"
                r)))
  ;; productive infinite first branch: interleaved mplus lets the fact run.
  (let ((answers (cl-kanren:run 10 (x)
                   (cl-kanren:conde
                     ((spino x))
                     ((cl-kanren:== 'done x))))))
    (format t "FAIR-PRODUCTIVE answers=~a done-reached=~a~%"
            answers (not (null (member 'done answers))))))

(defsection "append"
  ;; bidirectional append
  (format t "APPEND-LHS ~a~%"
          (cl-kanren:run 10 (xs) (cl-kanren:appendo xs '(c d) '(a b c d))))
  (format t "APPEND-RHS ~a~%"
          (cl-kanren:run 5 (q)
            (cl-kanren:fresh (ys zs)
              (cl-kanren:appendo '(a b) ys zs)
              (cl-kanren:== (list ys zs) q)))))

(defsection "neg"
  ;; bounded negative query over ground pairs via the ifte adapter.
  (format t "NEG not-edge(a,z)=~a not-edge(a,b)=~a~%"
          (not (null (cl-kanren:run 1 (q) (not-edgeo 'a 'z))))
          (not (null (cl-kanren:run 1 (q) (not-edgeo 'a 'b))))))

(defsection "constraints"
  ;; no constraint store of any kind ships with the library.
  (let ((pkg (find-package :cl-kanren)))
    (format t "CONSTRAINTS absent-from-probe disequality=~a domains=none~%"
            (and (find-symbol "DISEQUAL" pkg) t))))

(defsection "binarith"
  ;; binary arithmetic: no relational arithmetic exists in the library.
  (let ((pkg (find-package :cl-kanren)))
    (format t "BINARITH absent-from-probe pluso=~a numero=~a~%"
            (and (find-symbol "PLUSO" pkg) t)
            (and (find-symbol "NUMERO" pkg) t))))

(defsection "update"
  ;; finite fact update: remove (c d), re-query, restore, re-query.
  (setf *edges* (remove '(c d) *edges* :test #'equal))
  (let ((after-retract (cl-kanren:run 20 (x) (patho 'a x))))
    (setf *edges* (cons '(c d) *edges*))
    (let ((after-reassert (cl-kanren:run 20 (x) (patho 'a x))))
      (format t "UPDATE after-retract=~a after-reassert=~a~%"
              (dedup (sorted-answers after-retract))
              (dedup (sorted-answers after-reassert))))))

(defsection "fixpoint"
  ;; external bottom-up fixpoint adapter over the same edge fixture.
  (let ((edges '((a b) (b c) (c a) (c d)))
        (closure '((a b) (b c) (c a) (c d)))
        changed)
    (loop do (setf changed nil)
          do (dolist (e edges)
               (dolist (p closure)
                 (when (and (equal (second e) (first p))
                            (not (member (list (first e) (second p))
                                         closure :test #'equal)))
                   (push (list (first e) (second p)) closure)
                   (setf changed t))))
          while changed)
    (let ((from-a (sort (mapcar #'second
                                (remove-if-not (lambda (p) (eq (first p) 'a)) closure))
                        #'string<)))
      (format t "FIXPOINT-ADAPTER from-a=~a~%" from-a))))
