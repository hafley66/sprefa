;;; cl-gambol capability probe. Run:
;;;   GAMBOL_SRC=/path/to/cl-gambol sbcl --noinform --disable-debugger --script 2_PROBE.lisp
;;; Output contract: see 4_RESULTS.md.

(require :asdf)
(require :sb-posix)

(defpackage #:cl-gambol-probe
  (:use #:cl)
  (:export #:main #:*child-section*))

(in-package #:cl-gambol-probe)

(defparameter *gambol-src*
  (or (uiop:getenv "GAMBOL_SRC")
      (error "set GAMBOL_SRC to the cl-gambol checkout directory")))

(defparameter *script* (uiop:truename* (or *load-truename* (uiop:argv0))))

(asdf:load-asd (merge-pathnames "gambol.asd" (pathname (concatenate 'string *gambol-src* "/"))))
(asdf:load-system "gambol")

(defparameter *version* "0.03/d4d53a1e29a360f8aaab9134da89b8c6966fe16e")

(defun canon (x)
  "Canonical printable form of a solution value."
  (format nil "~s" x))

(defun sorted-answers (values)
  (sort (mapcar #'canon (copy-list values)) #'string<))

(defun collect-all (goal &optional (limit 500))
  "All solutions to GOAL, capped at LIMIT; second value says whether the cap hit."
  (let ((acc nil) (capped nil))
    (labels ((push-ans (b)
               (cond ((eq b t) (push t acc))
                     ((consp b) (push (cdr (first b)) acc))
                     (t nil))))
      (let ((first (gambol:pl-solve-one (list goal))))
        (when first (push-ans first))
        (loop repeat (max 0 (1- limit))
              for next = (gambol:pl-solve-next)
              while next
              do (push-ans next))
        (when (>= (length acc) limit) (setf capped t)))
      (values (nreverse acc) capped))))

(defmacro bounded (seconds &body body)
  "Run BODY under a wall-clock interrupt; on timeout return :timeout."
  `(handler-case (sb-ext:with-timeout ,seconds ,@body)
     (sb-ext:timeout () :timeout)))

(defparameter *child-section* nil
  "Set by the parent process for single-section child runs.")

(defun section (name)
  ;; Run a single probe section in a fresh child process with a hard
  ;; process timeout; cyclic bindings/recursion can overflow the stack
  ;; faster than an in-process timer can fire.
  (let ((out (bounded 60
               (with-output-to-string (s)
                 (sb-posix:setenv "PROBE_SECTION" name 1)
                 (uiop:run-program
                  (list "sbcl" "--noinform" "--disable-debugger"
                        "--script" (namestring *script*))
                  :output s :error-output s
                  :ignore-error-status t)))))
    out))

(defparameter *sections* (make-hash-table :test #'equal))
(defmacro defsection (name &body body)
  `(setf (gethash ,name *sections*) (lambda () ,@body)))

;;; ---------- fixture ----------

(defun load-fixture ()
  (gambol:clear-rules)
  (gambol:*- (edge a b))
  (gambol:*- (edge b c))
  (gambol:*- (edge c a))
  (gambol:*- (edge c d))
  ;; cyclic transitive-closure rule (termination probe target)
  (gambol:*- (path ?x ?y) (edge ?x ?y))
  (gambol:*- (path ?x ?y) (edge ?x ?z) (path ?z ?y))
  ;; acyclic duplicate-proof fixture
  (gambol:*- (dedge a b))
  (gambol:*- (dedge a c))
  (gambol:*- (dedge c b))
  (gambol:*- (dpath ?x ?y) (dedge ?x ?y))
  (gambol:*- (dpath ?x ?y) (dedge ?x ?z) (dpath ?z ?y))
  ;; insertion order deliberately differs from lexical order
  (gambol:*- (ordered a z))
  (gambol:*- (ordered a a))
  ;; the first FAIR rule yields forever, starving the later DONE fact under DFS
  (gambol:*- (spin a))
  (gambol:*- (spin ?x) (spin ?x))
  (gambol:*- (fair ?x) (spin ?x))
  (gambol:*- (fair done))
  ;; bidirectional append
  (gambol:*- (app nil ?ys ?ys))
  (gambol:*- (app (?x . ?xs) ?ys (?x . ?zs)) (app ?xs ?ys ?zs))
  ;; negation as failure
  (gambol:*- (nnot ?p) ?p (gambol::cut) (gambol::fail))
  (gambol:*- (nnot ?p)))

(defsection "unify"
  (load-fixture)
  (format t "UNIFY ~s~%"
          (gambol:pl-solve-one '((= (f ?x (g ?y)) (f a (g b)))))))

(defsection "occurs"
  ;; X = f(X): occurs check is absent in pl-bind; binding becomes cyclic and
  ;; reification (expand-logical-vars) loops or overflows the stack.
  (load-fixture)
  (let ((r (handler-case (bounded 5 (gambol:pl-solve-one '((= ?x (f ?x)))))
             (sb-kernel::control-stack-exhausted () :stack-exhausted))))
    (format t "OCCURS occurs-check=absent result=~a~%"
            (cond ((eq r :timeout)
                   "unify-succeeds-cyclically-reification-loops")
                  ((eq r :stack-exhausted)
                   "unify-succeeds-cyclically-reification-stack-overflows")
                  (r "unify-succeeds-cyclically-reified")
                  (t "failed")))))

(defsection "path"
  ;; cyclic transitive closure: DFS with no tabling does not terminate.
  (load-fixture)
  (multiple-value-bind (answers capped)
      (bounded 10 (collect-all '(path a ?x) 100))
    (format t "PATH answers=~a count=~d capped=~a~%"
            (remove-duplicates (sorted-answers answers) :test #'equal)
            (length answers)
            capped)))

(defsection "update"
  ;; fact update: retract dedge(c b), re-query, re-assert, re-query.
  (load-fixture)
  (bounded 10
    (gambol:pl-retract '((dedge c b)))
    (let ((before (collect-all '(dpath a ?x))))
      (gambol:pl-assert '((dedge c b)))
      (let ((after (collect-all '(dpath a ?x))))
        (format t "UPDATE after-retract=~a after-reassert=~a~%"
                (sorted-answers before) (sorted-answers after))))))

(defsection "extras"
  (load-fixture)
  (bounded 10
    ;; duplicate answers via two proofs
    (multiple-value-bind (dups _)
        (collect-all '(dpath a ?x) 50)
      (declare (ignore _))
      (format t "DUPES ~a~%" (sorted-answers dups)))
    ;; answer ordering = rule insertion order, DFS; preserve it in the output
    (format t "ORDER ~a~%"
            (mapcar #'canon (collect-all '(ordered a ?x) 10)))
    ;; the infinite first alternative prevents the later DONE fact from running
    (multiple-value-bind (answers capped)
        (collect-all '(fair ?x) 20)
      (format t "FAIR capped=~a done-reached=~a answers=~a count=~d~%"
              capped
              (not (null (member 'done answers)))
              (remove-duplicates (sorted-answers answers) :test #'equal)
              (length answers)))
    ;; bidirectional append
    (format t "APPEND-LHS ~a~%"
            (sorted-answers (collect-all '(app ?xs (c d) (a b c d)) 10)))
    (format t "APPEND-RHS ~a~%"
            (sorted-answers (collect-all '(app (a b) ?ys ?zs) 10)))
    ;; negation as bounded negative query
    (format t "NEG ~a ~a~%"
            (canon (collect-all '(nnot (edge a z)) 10))
            (canon (collect-all '(nnot (edge a b)) 10)))))

(defsection "fixpoint"
  ;; external bottom-up fixpoint adapter: iterate edge pairs to closure.
  (load-fixture)
  (let ((edges '((a b) (b c) (c a) (c d))) (closure (copy-list '((a b) (b c) (c a) (c d)))) changed)
    (loop do (setf changed nil)
          do (loop for (x y) in edges
                   do (loop for (y2 z) in closure
                            when (and (equal y y2)
                                      (not (member (list x z) closure :test #'equal)))
                              do (push (list x z) closure) (setf changed t)))
          while changed)
    (let ((from-a (sort (mapcar #'second (remove-if-not (lambda (p) (eq (first p) 'a)) closure)) #'string<)))
      (format t "FIXPOINT-ADAPTER from-a=~a~%" from-a))))

(defun child-run (name)
  (let ((sec (gethash name *sections*)))
    (funcall sec)))

(defun binary-bytes ()
  (let ((path (merge-pathnames "cl-gambol-lab"
                               (uiop:pathname-directory-pathname *script*))))
    (if (probe-file path)
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defun main ()
  (let ((section (uiop:getenv "PROBE_SECTION")))
    (if section
        (child-run section)
        (progn
          (format t "PROBE library=cl-gambol version=~a~%" *version*)
          (dolist (name (list "unify" "occurs" "path" "update" "extras" "fixpoint"))
            (princ (section name)))
          (format t "BINARY ~a~%" (binary-bytes))))))

(handler-case (main)
  (error (c) (format *error-output* "ERROR ~a~%" c) (uiop:quit 1))
  (:no-error (c) (declare (ignore c)) (uiop:quit 0)))
