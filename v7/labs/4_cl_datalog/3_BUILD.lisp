;; file: 3_BUILD.lisp
;; Standalone SBCL image attempt. cl-datalog loads, so the contract permits
;; one build. The image links the library in and prints its surface summary.
;; Output executable goes to .lab-cache/ (outside Git).

(require :asdf)

(defpackage #:cl-datalog-build
  (:use #:cl))
(in-package #:cl-datalog-build)

(defparameter *lab-dir* (make-pathname :defaults *load-truename* :name nil :type nil))

(asdf:load-asd "/tmp/cl-datalog-upstream/cl-datalog.asd")
(load (merge-pathnames ".lab-cache/.quicklisp/setup.lisp" *lab-dir*))
(funcall (find-symbol "QUICKLOAD" :ql) :trivial-types :silent t)
(asdf:initialize-source-registry
 '(:source-registry (:directory "/tmp/cl-datalog-upstream/") :inherit-configuration))
(asdf:load-system "cl-datalog")

(defun main ()
  (handler-case
      (progn
        (format t "PROBE library=cl-datalog version=0.0.1 commit=da2fb09a8c55cb9c4488358ee5dff4ab49ae473f~%")
        (format t "BUILT image=cl-datalog-lab loaded=(cl-datalog trivial-types) evaluator=absent source-load=~A compile=~A~%"
                (not (null (fboundp 'load)))
                (not (null (fboundp 'compile))))
        (uiop:quit 0))
    (error (c)
      (format *error-output* "ERROR ~A~%" c)
      (uiop:quit 1))))

(sb-ext:save-lisp-and-die
 (merge-pathnames ".lab-cache/cl-datalog-lab" *lab-dir*)
 :executable t
 :toplevel #'main
 :save-runtime-options t)
