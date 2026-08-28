;;; WAMCompiler bounded capability probe.
;;; Run with WAMCOMPILER_SRC=/path/to/wamcompiler and optionally
;;; WAMCOMPILER_BIN=/path/to/saved-image.

(require :asdf)

(defpackage #:wamcompiler-lab-probe
  (:use #:cl))

(in-package #:wamcompiler-lab-probe)

(defparameter *pin* "d46a2665734d1ed6c7e73ecda9c4e860631cd858")
(defparameter *version* "git/d46a266")
(defparameter *pinned-image-library* nil)
(defparameter *saved-image-path* nil)

(defparameter *source*
  (or (uiop:getenv "WAMCOMPILER_SRC")
      (error "set WAMCOMPILER_SRC to the WAMCompiler checkout directory")))

(defparameter *script*
  (or (uiop:getenv "PROBE_SCRIPT")
      (and *load-truename* (uiop:truename* *load-truename*))
      (error "cannot determine probe script path")))

(defun source-dir ()
  (uiop:ensure-directory-pathname
   (or (uiop:getenv "WAMCOMPILER_SRC") *source*)))

(defun verify-pin ()
  "Require the exact clean source checkout before loading any WAM code."
  (let* ((dir (namestring (source-dir)))
         (head (uiop:run-program (list "git" "-C" dir "rev-parse" "HEAD")
                                 :output '(:string :stripped t)))
         (status (uiop:run-program (list "git" "-C" dir "status" "--porcelain")
                                   :output '(:string :stripped t))))
    (unless (string= head *pin*)
      (error "WAMCompiler HEAD ~a does not match pin ~a" head *pin*))
    (unless (zerop (length status))
      (error "WAMCompiler checkout is dirty: ~a" status))
    head))

(defun library-loaded-p ()
  (and (fboundp 'cl-user::repl)
       (boundp 'cl-user::*dispatching-code-table*)
       (boundp 'cl-user::*trail-area*)))

(defun load-library ()
  (verify-pin)
  (cond ((library-loaded-p)
         (unless (equal *pinned-image-library* *pin*)
           (error "WAMCompiler was preloaded without pinned-image provenance")))
        (t
         (let ((*package* (find-package :cl-user)))
           (load (merge-pathnames "wamcompiler.lisp" (source-dir)))))))

(defmacro bounded (seconds &body body)
  `(handler-case
       (sb-ext:with-timeout ,seconds ,@body)
     (sb-ext:timeout () :timeout)))

(defun run-prolog (program &key (answers "a") (seconds 10))
  "Feed complete source to the documented REPL and capture its raw transcript."
  (handler-case
      (sb-ext:with-timeout seconds
        (let ((output (make-string-output-stream)))
          (let* ((answer-input (make-string-input-stream answers))
                 (*query-io* (make-two-way-stream answer-input output)))
            ;; The parser interns its tokens in *package*. WAMCompiler owns its
            ;; operator table in CL-USER, so each source parse must use that package.
            (let ((*package* (find-package :cl-user)))
              (cl-user::repl :silent t
                             :stream (make-string-input-stream program))))
          (get-output-stream-string output)))
    (sb-ext:timeout () :timeout)))

(defun transcript-lines (text)
  (if (eq text :timeout)
      '("timeout")
      (remove-if (lambda (line) (zerop (length line)))
                 (uiop:split-string text :separator '(#\Newline #\Return)))))

(defun quoted (text)
  (format nil "~s" text))

(defun prolog-lines (&rest lines)
  (format nil "~{~a~^~%~}~%" lines))

(defun with-prelude (&rest lines)
  (apply #'prolog-lines
         (format nil "?- consult('~a')."
                 (namestring (merge-pathnames "prelude.pl" (source-dir))))
         lines))

(defun binary-bytes ()
  (let ((path (or (uiop:getenv "WAMCOMPILER_BIN")
                  *saved-image-path*)))
    (if (and path (probe-file path))
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defparameter *fixture*
  (prolog-lines
   "edge(a,b)."
   "edge(b,c)."
   "edge(c,a)."
   "edge(c,d)."
   "path(X,Y):-edge(X,Y)."
   "path(X,Y):-edge(X,Z),path(Z,Y)."))

(defun section-unify ()
  (format t "UNIFY raw=~a~%"
          (quoted (run-prolog (with-prelude "?- f(X,g(Y))=f(a,g(b)).") :answers "yy"))))

(defun section-occurs ()
  (format t "OCCURS occurs-check=absent raw=~a~%"
          (quoted (run-prolog (with-prelude "?- X=f(X).") :answers "yy" :seconds 5))))

(defun section-path ()
  (format t "PATH raw=~a~%"
          (quoted (run-prolog (concatenate 'string (with-prelude) *fixture* (prolog-lines "?- path(a,X)."))
                               :answers "yy" :seconds 5))))

(defun section-path-bounded ()
  (format t "PATH-BOUND timeout-or-transcript=~a~%"
          (quoted (run-prolog (concatenate 'string (with-prelude) *fixture* (prolog-lines "?- path(a,X)."))
                               :answers "aaaa" :seconds 2))))

(defun section-append ()
  (format t "APPEND-LHS raw=~a~%"
          (quoted (run-prolog (with-prelude "?- append(X,[c,d],[a,b,c,d]).") :answers "yy")))
  (format t "APPEND-RHS raw=~a~%"
          (quoted (run-prolog (with-prelude "?- append([a,b],Y,Z).") :answers "yy"))))

(defun section-cut ()
  (format t "CUT raw=~a~%"
          (quoted (run-prolog (with-prelude "pick(a)." "pick(b):-!." "pick(c)." "?- pick(X).") :answers "ya"))))

(defun section-index ()
  (format t "INDEX wamcode=~a raw=~a~%"
          (quoted
           (with-output-to-string (out)
             (let ((*standard-output* out))
               (run-prolog (with-prelude "tag(a,one)." "tag(b,two)." "?- tag(b,X).") :answers "yy")
               (let ((*package* (find-package :cl-user)))
                 (cl-user::show-wamcode "tag" 2)))))
          (quoted (run-prolog (with-prelude "tag(a,one)." "tag(b,two)." "?- tag(b,X).") :answers "yy"))))

(defun section-update ()
  (format t "UPDATE raw=~a~%"
          (quoted (run-prolog (with-prelude
                               "item(a)."
                               "?- item(b), X=before."
                               "item(b)."
                               "?- item(b), X=after.")
                               :answers "yyyy"))))

(defun section-negation ()
  (format t "NEG raw=~a~%"
          (quoted (run-prolog (with-prelude "edge(a,b)." "?- \\+(edge(a,z))." "?- \\+(edge(a,b)).") :answers "yyy"))))

(defparameter *sections*
  '("unify" "occurs" "path" "path-bounded" "append" "cut" "index" "update" "negation"))

(defun run-section (name)
  (load-library)
  (let ((fn (intern (format nil "SECTION-~a" (string-upcase name))
                    :wamcompiler-lab-probe)))
    (funcall fn)))

(defun child-environment (name)
  (append
   (loop for line in (sb-ext:posix-environ)
         for equals = (position #\= line)
         when equals
           collect (cons (intern (subseq line 0 equals) :keyword)
                         (subseq line (1+ equals))))
   (list (cons :PROBE_SECTION name)
         (cons :WAMCOMPILER_SRC (namestring (source-dir))))))

(defun spawn-section (name)
  (multiple-value-bind (out err code)
      (uiop:run-program
       (if *pinned-image-library*
           (list (or (uiop:getenv "WAMCOMPILER_BIN")
                     *saved-image-path*
                     (error "saved WAMCompiler image path is unavailable")))
           (list "sbcl" "--noinform" "--disable-debugger" "--no-sysinit" "--no-userinit"
                 "--script" (namestring *script*)))
       :output :string
       :error-output :string
       :env (child-environment name)
       :ignore-error-status t)
    (if (zerop code)
        (princ out)
        ;; This implementation accepts X=f(X), then recursively converts the
        ;; cyclic WAM heap object for answer printing. SBCL exhausts its
        ;; control stack before an in-process wall timer can regain control.
        (if (and (string= name "occurs")
                 (search "CONTROL-STACK-EXHAUSTED" err))
            (format t "OCCURS occurs-check=absent result=unification-succeeds-cyclic-reification-stack-overflow~%")
            (error "section ~a failed with code ~a, stdout ~s, stderr ~s"
                   name code out err)))))

(defun main ()
  (let ((name (uiop:getenv "PROBE_SECTION")))
    (if name
        (run-section name)
        (progn
          (load-library)
          (format t "PROBE library=wamcompiler version=~a commit=~a~%" *version* *pin*)
          (dolist (section *sections*) (spawn-section section))
          (format t "BINARY ~a~%" (binary-bytes))))))

(if (uiop:getenv "PROBE_NOEXEC")
    (format *error-output* "loaded; probe execution suppressed~%")
    (handler-case
        (progn (main) (uiop:quit 0))
      (error (c)
        (format *error-output* "ERROR ~a~%" c)
        (uiop:quit 1))))
