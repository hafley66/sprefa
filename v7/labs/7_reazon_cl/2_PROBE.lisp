;;;; reazon-cl lab probe. SBCL 2.6.7, macOS arm64.
;;;;
;;;; Source run:
;;;;   REAZON_SRC=<checkout> QL_SETUP=<quicklisp setup.lisp> \
;;;;   sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 2_PROBE.lisp
;;;;
;;;; Image run (same env plus REAZON_LAB_BINARY=<path to the image>):
;;;;   ./reazon-cl-lab
;;;;
;;;; Provenance contract enforced here:
;;;;   1. the :reazon package must not exist before this file loads the pin;
;;;;   2. git at REAZON_SRC must be clean and at the pinned commit;
;;;;   3. the loaded system source must live under REAZON_SRC.

(eval-when (:compile-toplevel :load-toplevel :execute)
  (require :asdf))

(defparameter *pin* "3c4e9d916f2e621a3cc759f58ad778473f9da513")
(defparameter *pinned-image-library* nil)
(defparameter *trivia-archive-sha*
  "81f5eacce946f0ffd713f3ecfc97c92dcf5cf1773cbad12cbf378905e24d4913")
(defparameter *probe-path*
  (or *load-truename* (error "cannot determine probe source path")))

(defun file-sha256 (path)
  (first (uiop:split-string
          (uiop:run-program (list "shasum" "-a" "256" (namestring path))
                            :output '(:string :stripped t)))))

(defparameter *probe-sha* (file-sha256 *probe-path*))

(defparameter *ql-setup* (or (uiop:getenv "QL_SETUP")
                             (error "QL_SETUP not set")))
(defparameter *reazon-src* (or (uiop:getenv "REAZON_SRC")
                               (error "REAZON_SRC not set")))

(defun fail (fmt &rest args)
  (format *error-output* "PROVENANCE-FAIL ~?~%" fmt args)
  (uiop:quit 1))

(defun current-reazon-src ()
  (or (uiop:getenv "REAZON_SRC") *reazon-src*))

(defun current-ql-setup ()
  (or (uiop:getenv "QL_SETUP") *ql-setup*))

(defun ql-root ()
  (make-pathname :defaults (pathname (current-ql-setup)) :name nil :type nil))

(defun verify-trivia-archive ()
  (let ((archive (merge-pathnames
                  "dists/quicklisp/archives/trivia-20260101-git.tgz"
                  (ql-root))))
    (unless (and (probe-file archive)
                 (string= (file-sha256 archive) *trivia-archive-sha*))
      (fail "Trivia archive is missing or has the wrong SHA-256: ~a" archive))))

(defun verify-git-pin ()
  (let* ((root (current-reazon-src))
         (head (string-trim '(#\Newline #\Return)
                            (uiop:run-program `("git" "-C" ,root "rev-parse" "HEAD")
                                              :output '(:string :stripped t))))
         (dirty (uiop:run-program `("git" "-C" ,root "status" "--porcelain")
                                  :output :string)))
  (unless (string= head *pin*)
    (fail "checkout HEAD ~a != pin ~a" head *pin*))
  (unless (and dirty (string= (string-trim '(#\Space #\Newline #\Return) dirty) ""))
      (fail "checkout is dirty: ~s" dirty))))

(defun verify-loaded-source ()
  (let ((src (namestring (asdf:component-pathname (asdf:find-system :reazon-cl))))
        (root (namestring (truename (current-reazon-src)))))
    (unless (uiop:string-prefix-p root src)
      (fail "loaded system lives at ~a, not under ~a" src root))))

(defun verify-loaded-trivia ()
  (let ((src (namestring (asdf:component-pathname (asdf:find-system :trivia))))
        (root (namestring
               (truename
                (merge-pathnames
                 "dists/quicklisp/software/trivia-20260101-git/"
                 (ql-root))))))
    (unless (uiop:string-prefix-p root src)
      (fail "loaded Trivia system lives at ~a, not under ~a" src root))))

;; A fresh source run rejects a preloaded package. A saved image carries the
;; exact pin in *pinned-image-library* and rechecks Git and ASDF provenance at
;; every toplevel invocation.
(when (find-package :reazon)
  (fail "package :reazon already exists before pinned load"))

(verify-git-pin)
(verify-trivia-archive)
(load (current-ql-setup))

(asdf:load-asd (merge-pathnames "reazon-cl.asd"
                                (uiop:ensure-directory-pathname
                                 (truename *reazon-src*))))
(asdf:load-system :reazon-cl)

(verify-loaded-source)
(verify-loaded-trivia)
(setf *pinned-image-library* *pin*)

(defun verify-provenance ()
  (verify-git-pin)
  (unless (and (find-package :reazon)
               (equal *pinned-image-library* *pin*))
    (fail "loaded Reazon does not carry pin ~a" *pin*))
  (verify-trivia-archive)
  (verify-loaded-source)
  (verify-loaded-trivia))

;;(format t "PROVENANCE-OK commit=~a~%" *pin*)

(defpackage :reazon-lab
  (:use :cl)
  (:import-from :reazon #:== #:run #:run* #:defrel #:conj-2 #:disj-2 #:fresh
                #:conde #:appendo #:*occurs-check* #:circular-query))
(in-package :reazon-lab)

(defparameter *false-sym* (find-symbol "!U" :reazon))
(defparameter *true-sym* (find-symbol "!S" :reazon))
(defun goal-fail () (symbol-function *false-sym*))
(defun goal-succeed () (symbol-function *true-sym*))

;; ---------------------------------------------------------------- terms ----

(defun term->string (x)
  (cond ((symbolp x)
         (if (eq (symbol-package x) (find-package :reazon.reify))
             (format nil "?~a" (symbol-name x))
             (string x)))
        ((atom x) (princ-to-string x))
        ((consp x) (format nil "(~{~a~^ ~})" (mapcar #'term->string x)))
        (t (princ-to-string x))))

(defun sort-strings (list)
  (sort (copy-seq list) #'string<))

(defmacro answers->strings (n &body goals)
  `(mapcar #'term->string (run ,n q ,@goals)))

;; --------------------------------------------------------- fact adapter ----
;; Reazon has no fact store; a fact set is rebuilt as a disjunction of
;; (== x from) (== y to) goals over a host-side list. Update = setq + rebuild.

(defparameter *edges* '((a . b) (b . c) (c . a) (c . d)))

(defun edgeo (x y)
  (let ((clauses (mapcar (lambda (e)
                           (conj-2 (== x (car e)) (== y (cdr e))))
                         *edges*)))
    (reduce (lambda (acc clause) (disj-2 acc clause))
            clauses
            :initial-value (goal-fail))))

;; Cyclic path closure with an explicit termination mechanism: a ground depth
;; bound carried as an argument and decremented at call time.
(defun patho (x y depth)
  (disj-2
   (edgeo x y)
   (if (zerop depth)
       (goal-fail)
       (fresh (z)
         (conj-2 (edgeo x z)
                 (patho z y (1- depth)))))))

;; ------------------------------------------------------------- probes -----
;; defrel delays the body in a thunk. These mirror the upstream test's
;; productive and unproductive infinite relations.
(reazon:defrel inf-alwayso ()
  (reazon:disj (goal-succeed) (inf-alwayso)))

(reazon:defrel inf-nevero ()
  (reazon:disj (goal-fail) (inf-nevero)))

(defun probe ()
  (cl-user::verify-provenance)
  (format t "PROBE library=reazon-cl version=3c4e9d916f2e621a3cc759f58ad778473f9da513~%")
  (format t "PROVENANCE trivia-version=0.1 trivia-archive-sha=~a probe-sha=~a~%"
          cl-user::*trivia-archive-sha* cl-user::*probe-sha*)

  ;; nested unification: f(q, g(r)) = f(a, g(b))
  (let ((ans (run 1 (u v)
               (== (list u (list 'g v)) (list 'a (list 'g 'b))))))
    (format t "UNIFY u=~a v=~a~%"
            (term->string (first (first ans)))
            (term->string (second (first ans)))))

  ;; occurs check policy: X = (X) with *occurs-check* at its default
  (format t "OCCURS occurs-check=~a policy=dynamic-default result=~a~%"
          *occurs-check*
          (handler-case
              (progn (run 1 x (== x (list x)))
                     'unify-succeeded)
            (circular-query () 'circular-query-error)
            (error (c) (format nil "error:~a" (type-of c)))))

  ;; raw answer ordering: host list is z->b then a->b; query (edge ?x b)
  (let ((*edges* '((z . b) (a . b))))
    (format t "ORDER raw=~{~a~^ ~} sorted=~{~a~^ ~}~%"
            (answers->strings nil (edgeo q 'b))
            (sort-strings (answers->strings nil (edgeo q 'b)))))

  ;; bidirectional append
  (format t "APPEND-LHS ~{~a~^ ~}~%" (answers->strings 1 (appendo q '(c d) '(a b c d))))
  (format t "APPEND-RHS ~{~a~^ ~}~%" (answers->strings 1 (appendo '(a b) q '(a b c d))))

  ;; Productive recursive streams interleave with a later finite answer. An
  ;; unproductive recursive stream lets the later answer through, then blocks
  ;; a request for another answer; that request has a one-second wall bound.
  (let ((ans (run 20 q (disj-2 (inf-alwayso) (== q 'done)))))
    (format t "FAIR-PRODUCTIVE cap=20 answers=~a count=~a done-reached=~a~%"
            (if (member "DONE" ans :test #'string=) 'YES 'NO)
            (length ans)
            (if (member "DONE" ans :test #'string=) 'T 'NIL)))
  (let ((first (run 1 q (disj-2 (inf-nevero) (== q 'done))))
        (second (handler-case
                    (sb-ext:with-timeout 1
                      (run 2 q (disj-2 (inf-nevero) (== q 'done)))
                      'unexpected-return)
                  (sb-ext:timeout () 'timeout))))
    (format t "FAIR-STARVE first=~a second=~a~%" first second))

  ;; cyclic path closure, duplicates, sorted answers (depth bound 4)
  (let* ((raw (answers->strings nil (patho 'a q 4))))
    (format t "PATH raw=~a~%" raw)
    (format t "PATH-SORTED ~a count=~a~%" (sort-strings raw) (length raw))
    (format t "DUPES count=~a sorted-unique=~a~%"
            (- (length raw) (length (remove-duplicates raw :test #'string=)))
            (sort-strings (remove-duplicates raw :test #'string=))))

  ;; bounded negative query: which fixture node has no outgoing edge?
  ;; Bound = run 1 per candidate.
  (format t "NEG ~a~%"
          (remove-if (lambda (node)
                       (answers->strings 1 (edgeo node q)))
                     '(a b c d)))

  ;; finite fact update: retract (c . d), re-assert it
  (let ((with-d *edges*))
    (setq *edges* (remove '(c . d) with-d :test #'equal))
    (format t "UPDATE after-retract=~a~%"
            (sort-strings (answers->strings nil (patho 'a q 4))))
    (setq *edges* with-d)
    (format t "UPDATE after-reassert=~a~%"
            (sort-strings (answers->strings nil (patho 'a q 4)))))

  ;; constraints: nothing exported, nothing in src/reazon.lisp
  (format t "CONSTRAINTS absent-from-probe~%")

  ;; The caller supplies the executable path; the probe measures the file.
  (format t "BINARY ~a~%"
          (let ((path (uiop:getenv "REAZON_LAB_BINARY")))
            (if (and path (probe-file path))
                (with-open-file (stream path :element-type '(unsigned-byte 8))
                  (file-length stream))
                "blocked:not-built"))))

;; build mode suppresses probe execution (set by 3_BUILD.lisp)
(if (equal (uiop:getenv "REAZON_LAB_SUPPRESS_PROBE") "1")
    (format t "PROBE-SUPPRESSED build-mode~%")
    (progn (probe) (uiop:quit 0)))
