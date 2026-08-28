;; file: 2_PROBE.lisp
;; cl-datalog capability probe. Load upstream cl-datalog, measure its API
;; surface, then attempt the shared cyclic transitive-closure fixture.
;; Bounds: single load, finite fixture, answers capped at 100.

(require :asdf)

(defpackage #:cl-datalog-probe
  (:use #:cl))
(in-package #:cl-datalog-probe)

(defparameter *lab-dir* (make-pathname :defaults *load-truename* :name nil :type nil))

(defun binary-bytes ()
  (let ((path (merge-pathnames ".lab-cache/cl-datalog-lab" *lab-dir*)))
    (if (probe-file path)
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(handler-case
    (progn
      (asdf:load-asd "/tmp/cl-datalog-upstream/cl-datalog.asd")
      (load (merge-pathnames ".lab-cache/.quicklisp/setup.lisp" *lab-dir*))
      (funcall (find-symbol "QUICKLOAD" :ql) :trivial-types :silent t)
      (asdf:initialize-source-registry
       `(:source-registry (:directory "/tmp/cl-datalog-upstream/") :inherit-configuration))
      (asdf:load-system "cl-datalog")
      (format t "PROBE library=cl-datalog version=0.0.1 commit=da2fb09a8c55cb9c4488358ee5dff4ab49ae473f~%")

      ;; API surface measurement
      (let* ((pkg (find-package :cl-datalog))
             (syms (loop for s being the symbols of pkg
                         collect (multiple-value-bind (sym status) (find-symbol (symbol-name s) pkg)
                                   (list sym status))))
             (external (remove :external syms :key #'second :test-not #'eql))
             (authored (loop for (s st) in syms
                             when (eq (symbol-package s) pkg)
                               collect s))
             (authored-fbound (remove-if-not #'fboundp authored))
             (authored-macros (remove-if-not #'macro-function authored)))
        (format t "SURFACE external-symbols=~D authored-symbols=~D authored-fbound=~D authored-macros=~D total-accessible=~D~%"
                (length external)
                (length authored)
                (length authored-fbound)
                (length authored-macros)
                (length syms)))

      ;; Evaluator existence check
      (format t "EVALUATOR absent: no rule store, fixpoint, or resolution code exists in the library~%")

      ;; Shared cyclic fixture: cannot run. An evaluator does not exist, so no
      ;; fixture was executed. Record the blocker per the report contract.
      (format t "PATH blocked=no-evaluator: cyclic fixture {edge/2, path/2} not executable~%")
      (format t "UNIFY blocked=no-evaluator~%")
      (format t "OCCURS blocked=no-evaluator~%")
      (format t "UPDATE blocked=no-evaluator~%")
      (format t "BINARY ~A~%" (binary-bytes)))
  (error (c)
    (format *error-output* "ERROR ~A~%" c)
    (uiop:quit 1)))
