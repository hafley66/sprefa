;; file: 2_PROBE.lisp
;; cl-kanren (cage/Codeberg) capability probe. Run:
;;   KANREN_SRC=/path/to/cl-kanren PROBE_SCRIPT=/path/to/2_PROBE.lisp \
;;   sbcl --noinform --disable-debugger --no-sysinit --no-userinit \
;;     --load /private/tmp/sprefa-lab8-cache/.quicklisp/setup.lisp --script 2_PROBE.lisp
;; The fixture/sections live in 2a_FIXTURE.lisp, loaded after the pinned
;; library so its symbols resolve. Divergent sections run in fresh child
;; processes with hard wall bounds.

(require :asdf)

(defpackage #:cl-kanren-probe
  (:use #:cl))

(in-package #:cl-kanren-probe)

(defparameter *pin* "ad40ba1abb909f84f56ec503d225d1968ee82912")
(defparameter *version* "0.1.0")
(defparameter *pinned-image-library* nil)
(defvar *sections*)

(defparameter *kanren-src*
  (or (uiop:getenv "KANREN_SRC")
      (error "set KANREN_SRC to the cl-kanren checkout directory")))

(defparameter *script*
  (or (uiop:getenv "PROBE_SCRIPT")
      (and *load-truename* (uiop:truename* *load-truename*))
      (error "cannot determine probe script path")))

(defparameter *ql-setup*
  (or (uiop:getenv "QL_SETUP")
      "/private/tmp/sprefa-lab8-cache/.quicklisp/setup.lisp"))

(defparameter *bin-path*
  (uiop:getenv "KANREN_BIN"))

(defun current-kanren-src ()
  (or (uiop:getenv "KANREN_SRC") *kanren-src*))

(defun current-script ()
  (or (uiop:getenv "PROBE_SCRIPT") *script*))

(defun current-ql-setup ()
  (or (uiop:getenv "QL_SETUP") *ql-setup*))

(defun verify-pin ()
  "Enforce the exact clean Git pin at probe load."
  (let* ((head (string-trim '(#\newline #\return)
                            (uiop:run-program
                             (list "git" "-C" (current-kanren-src) "rev-parse" "HEAD")
                             :output '(:string :stripped t))))
         (status (string-trim '(#\space #\newline #\return)
                              (uiop:run-program
                               (list "git" "-C" (current-kanren-src) "status" "--porcelain")
                               :output '(:string :stripped t)))))
    (unless (string= head *pin*)
      (error "checkout HEAD ~a does not match pin ~a" head *pin*))
    (unless (zerop (length status))
      (error "checkout tree is not clean: ~a" status))
    head))

(defun load-library ()
  (verify-pin)
  (if (find-package :cl-kanren)
      (if (equal *pinned-image-library* *pin*)
          (format *error-output* "library already in verified pinned image; skipping reload~%")
          (error "CL-KANREN was preloaded without pinned-image provenance"))
      (progn
        (asdf:load-asd (merge-pathnames
                        "cl-kanren.asd"
                        (uiop:ensure-directory-pathname (current-kanren-src))))
        (asdf:load-system "cl-kanren"))))

(defun sorted-answers (values)
  (sort (mapcar (lambda (x) (format nil "~s" x)) (copy-list values)) #'string<))

(defun dedup (xs)
  (remove-duplicates xs :test #'string=))

(defun fixture-path ()
  (merge-pathnames "2a_FIXTURE.lisp"
                   (make-pathname :defaults (current-script))))

(defun surface ()
  (let ((external 0) (total 0) (fbound 0) (macros 0))
    (do-symbols (s (find-package :cl-kanren))
      (multiple-value-bind (sym status) (find-symbol (symbol-name s) :cl-kanren)
        (declare (ignore sym))
        (incf total)
        (when (eq status :external) (incf external))))
    (do-external-symbols (s :cl-kanren)
      (when (fboundp s)
        (if (macro-function s) (incf macros) (incf fbound))))
    (format t "SURFACE external-symbols=~d external-fbound=~d external-macros=~d total-accessible=~d~%"
            external fbound macros total)))

(defun binary-bytes ()
  (let ((path (or (uiop:getenv "KANREN_BIN") *bin-path*)))
    (if (and path (probe-file path))
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defun child-run (name)
  (load-library)
  (load (fixture-path))
  (let ((sec (gethash name *sections*)))
    (if sec (funcall sec) (error "unknown section ~a" name))))

(defun spawn-section (name)
  ;; fresh child process; the child bounds divergent goals with in-process
  ;; timers, and the 120s process cap catches anything that slips past.
  (handler-case
      (sb-ext:with-timeout 120
        (multiple-value-bind (out err code)
            (uiop:run-program
             (list "sbcl" "--noinform" "--disable-debugger" "--no-sysinit" "--no-userinit"
                   "--load" (current-ql-setup)
                   "--script" (namestring (current-script)))
             :output :string :error-output :string
             :env (append (loop for line in (sb-ext:posix-environ)
                                for eq = (position #\= line)
                                when eq collect
                                (cons (intern (subseq line 0 eq) :keyword)
                                      (subseq line (1+ eq))))
                          (list (cons :PROBE_SECTION name)
                                (cons :KANREN_SRC (current-kanren-src))))
             :ignore-error-status t)
          (if (zerop code)
              (princ out)
              (error "section ~a child failed with code ~a, stdout ~s, stderr ~s"
                     name code out err))))
    (sb-ext:timeout () (error "section ~a exceeded 120 seconds" name))))

(defun parent-run ()
  (let ((commit (verify-pin)))
    (format t "PROBE library=cl-kanren version=~a commit=~a~%" *version* commit)
    (load-library)
    (surface)
    (dolist (name (list "unify" "occurs" "path" "path-unbounded" "dupes" "order"
                        "fair" "append" "neg" "constraints" "binarith"
                        "update" "fixpoint"))
      (spawn-section name))
    (format t "BINARY ~a~%" (binary-bytes))))

(defun main ()
  (let ((section (uiop:getenv "PROBE_SECTION")))
    (if section
        (child-run section)
        (parent-run))))

(if (uiop:getenv "PROBE_NOEXEC")
    (format *error-output* "loaded; probe execution suppressed~%")
    (handler-case (main)
      (error (c)
        (format *error-output* "ERROR ~a~%" c)
        (uiop:quit 1))
      (:no-error (c)
        (declare (ignore c))
        (uiop:quit 0))))
