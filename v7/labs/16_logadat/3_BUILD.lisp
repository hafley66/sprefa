;;;; Lab 16 build: standalone SBCL image embedding logadat + a probe.
;;;; Run: LOGADAT_SRC=<checkout> LOGADAT_COMMIT=<sha> LOGADAT_OUT=<path> \
;;;;   sbcl --noinform --disable-debugger --script 3_BUILD.lisp

(require :sb-posix)
(defpackage #:logadat-build
  (:use #:cl))
(in-package #:logadat-build)

(defparameter *src* (or (sb-posix:getenv "LOGADAT_SRC")
                        "/private/tmp/sprefa-v7-lab16/logadat"))
(defparameter *out* (or (sb-posix:getenv "LOGADAT_OUT")
                        "/private/tmp/sprefa-v7-lab16/logadat-lab-image"))
(defparameter *commit* (or (sb-posix:getenv "LOGADAT_COMMIT") "unpinned"))

;; Load the single vendored source file into this package. The library has no
;; in-package form, so its symbols (including the compr/compr-pm macros read by
;; rule-to-compr at runtime) all live in logadat-build.
(load (merge-pathnames "logadat.lisp"
                       (pathname (concatenate 'string *src* "/"))))

(defun sorted-unique (rows)
  (sort (remove-duplicates rows :test #'equal) #'string<
        :key (lambda (r) (format nil "~S" r))))

(defun fmt (rows) (format nil "(~{(~{~A~^ ~})~^ ~})" rows))

(defun pred-value (preds name)
  (current-value (nth-value 0 (gethash name preds))))

(defun fixpoint-with-bound (idb edb max-rounds)
  "Source-derived copy of naive-evaluation (logadat.lisp:312-316) with an
explicit round bound, using the library's own rewrite/eval/equality code."
  (let ((current idb) (rounds 0))
    (loop
      (incf rounds)
      (when (> rounds max-rounds)
        (error "FIXPOINT-BOUND-EXCEEDED after ~D rounds" max-rounds))
      (let ((new-preds (eval-preds (rewrite-preds-rules current edb))))
        (when (predicate= current new-preds)
          (return (values new-preds rounds)))
        (setf current new-preds)))))

(defun probe-core ()
  (format t "PROBE library=logadat version=~A image=built~%" *commit*)
  (let* ((edb (collect-facts '((edge (a b) (b c) (c a) (c d))) (make-hash-table)))
         (idb (collect-preds '((path (x y) (in (x y) edge))
                           (path (x y) (in (x z) edge) (in (z y) path)))
                          (make-hash-table))))
    (multiple-value-bind (preds rounds)
        (fixpoint-with-bound idb edb 100)
      (format t "PATH full=~A rounds=~D~%"
              (fmt (sorted-unique (pred-value preds 'path))) rounds))))

(defun main ()
  (handler-case
      (progn (probe-core) (sb-ext:exit :code 0))
    (error (c)
      (format *error-output* "ERROR ~A~%" c)
      (sb-ext:exit :code 1))))

(sb-ext:save-lisp-and-die *out* :executable t :toplevel #'main)
