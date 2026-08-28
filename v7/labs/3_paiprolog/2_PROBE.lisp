;;;; paiprolog capability probe
;;;; Run: PAIPROLOG_SRC=<checkout> sbcl --noinform --disable-debugger --script 2_PROBE.lisp

(require :asdf)

(defpackage #:paiprolog-lab
  (:use #:cl)
  (:export #:run-probe))

(in-package #:paiprolog-lab)

(defparameter *version* "012d6bb255d8af7f1c8b1d061dcd8a474fb3b57a")

(eval-when (:compile-toplevel :load-toplevel :execute)
  (let ((src (uiop:getenv "PAIPROLOG_SRC")))
    (unless src
      (format *error-output* "ERROR PAIPROLOG_SRC not set~%")
      (uiop:quit 1))
    (let ((actual (string-trim '(#\Space #\Tab #\Newline #\Return)
                               (uiop:run-program
                                (list "git" "-C" src "rev-parse" "HEAD")
                                :output :string))))
      (unless (string= actual *version*)
        (error "PAIPROLOG_SRC commit ~a does not match pinned commit ~a"
               actual *version*)))
    (asdf:load-asd (merge-pathnames "paiprolog.asd" (pathname (concatenate 'string src "/"))))
    (asdf:load-system "paiprolog")))

(import 'paiprolog:<- :paiprolog-lab)
(import 'paiprolog:<-- :paiprolog-lab)

(defun sym-sort (list)
  (sort (copy-list list) #'string< :key #'(lambda (x) (format nil "~s" x))))

;; ---------- interpreter receipts ----------

(defun interp-nested-unify ()
  ;; nested compound unification through the interpreter unify
  (format nil "~s" (paiprolog::unifier '(f ?x (g a)) '(f b (g ?y)))))

(defun interp-occurs ()
  ;; interpreter occurs-check default is t: X = f(X) must fail.
  ;; With *occurs-check* nil the same unification creates a cyclic binding
  ;; (verified: subst-bindings on the result diverges, so only the
  ;; success/fail status is reported here).
  (let ((on (paiprolog::unify '?x (list 'f '?x) paiprolog::no-bindings))
        (off (let ((paiprolog::*occurs-check* nil))
               (paiprolog::unify '?x (list 'f '?x) paiprolog::no-bindings))))
    (format nil "interp-default(t)=~s interp-nil=~s"
            (eq on paiprolog::fail) (not (eq off paiprolog::fail)))))

;; ---------- compiled receipts ----------

(defun collect-1 (goal-fn)
  "Run a paiprolog query form, return the collected list."
  goal-fn)

(defun split-string (s)
  (loop for start = 0 then (1+ end)
        for end = (position #\Newline s :start start)
        collecting (subseq s start end)
        while end))

(defun binary-bytes ()
  (let ((path (uiop:getenv "PAIPROLOG_LAB_BINARY")))
    (if (and path (probe-file path))
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defun trace-record-p (line)
  (let* ((s (string-left-trim " " line))
         (n (length s)))
    (and (> n 2)
         (digit-char-p (char s 0))
         (char= #\: (char s 1)))))

(defun trace-return-p (line)
  (and (trace-record-p line)
       (search " returned " line)))

(defun run-probe ()
  (format t "PROBE library=paiprolog version=~a~%" *version*)

  ;; nested term unification, interpreter (occurs-check on)
  (format t "UNIFY ~a~%" (interp-nested-unify))
  (format t "OCCURS ~a~%" (interp-occurs))

  ;; compiled "occurs-check" predicate: also implemented with destructive
  ;; unify! (no occurs check). X = f(X) creates a cyclic binding, so any
  ;; deref/print of X diverges. Sentinels avoid derefing the cyclic term.
  (let ((r1 (paiprolog:prolog-first (?v)
              (paiprolog:unify-with-occurs-check ?v (f ?v))
              (paiprolog:lisp (return-from paiprolog:prolog 'cyclic-binding-created))))
        (r2 (paiprolog:prolog-first (?v)
              (paiprolog:unify-with-occurs-check ?v (f a))
              (paiprolog:lisp (return-from paiprolog:prolog 'bound-ok)))))
    (format t "OCCURS-CHECKED compiled unify-with-occurs-check: cyclic-case=~s ground-case=~s~%" r1 r2))

  ;; compile-unify rejects this direct self-occurrence while compiling the
  ;; anonymous query, before the destructive runtime unifier is reached.
  (let ((r (paiprolog:prolog-first (?x)
             (paiprolog:= ?x (f ?x))
             (paiprolog:lisp (return-from paiprolog:prolog 'cyclic-binding-created)))))
    (format t "OCCURS-COMPILED default-compiled-= ~s~%" r))

  ;; ----- fixture: edge facts and bounded path adapter -----
  (<- (edge a b))
  (<- (edge b c))
  (<- (edge c a))
  (<- (edge c d))

  ;; Unbounded cyclic closure and its depth-bounded adapter.
  (<- (path ?x ?y) (edge ?x ?y))
  (<- (path ?x ?y) (edge ?x ?z) (path ?z ?y))
  (<- (pathd ?x ?y ?d) (edge ?x ?y))
  (<- (pathd ?x ?y ?d)
      (paiprolog:> ?d 0)
      (paiprolog:is ?d1 (- ?d 1))
      (edge ?x ?z)
      (pathd ?z ?y ?d1))

  ;; duplicate-answer count before dedup
  (let* ((raw (paiprolog:prolog-collect (?y) (pathd a ?y 4)))
         (dedup (sym-sort (remove-duplicates raw :test #'equal))))
    (format t "PATH raw=~s raw-count=~d sorted=~s~%PATH-MECH adapter=depth-bound engine=dfs-sld-compiled no-tabling~%"
            raw (length raw) dedup))
  (let ((result
          (handler-case
              (sb-ext:with-timeout 0.001
                (paiprolog:prolog-collect (?y) (path a ?y)))
            (sb-ext:timeout () :timed-out))))
    (format t "PATH-CYCLE unbounded=~(~a~)~%" result))

  ;; A divergent first clause blocks the finite second clause under DFS.
  ;; SBCL's timeout bounds the probe without changing the engine's search.
  (<- (spin) (spin))
  (<- (starve blocked) (spin))
  (<- (starve reachable))
  (let ((result
          (handler-case
              (sb-ext:with-timeout 0.1
                (paiprolog:prolog-first (?x) (starve ?x)))
            (sb-ext:timeout () :starved))))
    (format t "FAIR dfs-left-branch=~(~a~) later-answer=reachable~%" result))

  ;; ----- cut -----
  (<- (first-edge ?x) (edge ?x ?) paiprolog:!)
  (let ((r (paiprolog:prolog-collect (?x) (first-edge ?x))))
    (format t "CUT answers=~s (cut commits first clause, drops the rest)~%" r))

  ;; ----- bidirectional append -----
  (<- (app () ?ys ?ys))
  (<- (app (?x . ?xs) ?ys (?x . ?zs)) (app ?xs ?ys ?zs))
  (let ((splits (paiprolog:prolog-collect (?u ?v) (app ?u ?v (a b))))
        (suffix (paiprolog:prolog-collect (?u) (app ?u (b c) (a b c)))))
    (format t "APPEND as-split=~s~%APPEND as-prefix=~s~%"
            (sym-sort splits) suffix))

  ;; ----- duplicates from the adapter path -----
  (let ((r (paiprolog:prolog-collect (?y) (pathd b ?y 2))))
    (format t "DUPES raw=~s sorted-dedup=~s~%" r (sym-sort (remove-duplicates r :test #'equal))))

  ;; ----- update: retract single fact (edge c d), recompile happens lazily -----
  (paiprolog::retract-clause '((edge c d)))
  (let ((r (paiprolog:prolog-collect (?y) (pathd a ?y 4))))
    (format t "UPDATE after-retract sorted=~s~%" (sym-sort (remove-duplicates r :test #'equal))))
  ;; quek's <-- replaces ALL same-arity clauses, so sequential <-- calls leave
  ;; only the last clause; verified: after (<-- e1)(<-- e2) only e2 remains.
  (<- (tmpf 1))
  (<-- (tmpf 2))
  (format t "UPDATE-MECH <-- replace-all tmpf-clauses=~s~%" (paiprolog::get-clauses 'tmpf))

  ;; ----- bounded negation as failure via PAIP if-then -----
  (<- (nodes a))
  (<- (nodes b))
  (<- (nodes c))
  (<- (nodes d))
  (<- (unreach ?x) (nodes ?x) (paiprolog:fail-if (pathd a ?x 4)))
  (let ((r (paiprolog:prolog-collect (?x) (unreach ?x))))
    (format t "NEGATION bounded-naf sorted=~s~%" (sym-sort (remove-duplicates r :test #'equal))))

  ;; ----- interpreter unification trace (bounded: one edge goal) -----
  (let ((*trace-output* (make-string-output-stream)))
    (trace paiprolog::unify)
    (paiprolog::prove-all '((edge a ?x)) paiprolog::no-bindings)
    (untrace paiprolog::unify)
    (let* ((text (get-output-stream-string *trace-output*))
           (records (remove-if-not #'trace-record-p (split-string text)))
           (returns (count-if #'trace-return-p records))
           (calls (- (length records) returns)))
      (format t "TRACE interp-unify calls=~d returns=~d records=~d (raw trace kept in 4_RESULTS.md)~%"
              calls returns (length records))
      (format t "TRACE-RAW-BEGIN~%~aTRACE-RAW-END~%" text)))
  (format t "BINARY ~a~%" (binary-bytes)))

(handler-case
    (when (uiop:getenv "PAIPROLOG_LAB_NO_RUN")
      (format t "BUILD-LOAD probe code loaded, run suppressed~%"))
  (error () nil))
(when (null (uiop:getenv "PAIPROLOG_LAB_NO_RUN"))
  (handler-case (run-probe)
    (error (c)
      (format *error-output* "ERROR ~a~%" c)
      (uiop:quit 1))))
