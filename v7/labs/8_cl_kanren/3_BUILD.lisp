;; file: 3_BUILD.lisp
;; Standalone SBCL image attempt. Probe execution is suppressed during the
;; build via PROBE_NOEXEC; the image's toplevel is the probe's own main.
;; Output path comes from the KANREN_OUT environment variable (external to Git).

(require :asdf)

(defpackage #:cl-kanren-build
  (:use #:cl))

(in-package #:cl-kanren-build)

(defparameter *lab-dir*
  (make-pathname :defaults (or *load-truename* (uiop:argv0)) :name nil :type nil))

(defparameter *kanren-src* (uiop:getenv "KANREN_SRC"))
(defparameter *ql-setup* (or (uiop:getenv "QL_SETUP")
                             (error "set QL_SETUP to Quicklisp setup.lisp")))
(defparameter *out* (or (uiop:getenv "KANREN_OUT")
                        (error "set KANREN_OUT to the external output path")))

(defun build ()
  (load *ql-setup*)
  (setf (uiop:getenv "PROBE_NOEXEC") "1")
  (load (merge-pathnames "2_PROBE.lisp" *lab-dir*))
  (funcall (symbol-function
            (find-symbol "VERIFY-PIN" "CL-KANREN-PROBE")))
  (funcall (symbol-function
            (find-symbol "LOAD-LIBRARY" "CL-KANREN-PROBE")))
  (setf (symbol-value
         (find-symbol "*PINNED-IMAGE-LIBRARY*" "CL-KANREN-PROBE"))
        (symbol-value
         (find-symbol "*PIN*" "CL-KANREN-PROBE")))
  (format t "loaded; probe execution suppressed during build~%")
  (sb-ext:save-lisp-and-die
   *out*
   :executable t
   ;; find-symbol at runtime: the probe package exists only after the file load.
   :toplevel (lambda ()
               (handler-case
                   (progn
                     (funcall (symbol-function
                               (find-symbol "MAIN" "CL-KANREN-PROBE")))
                     (uiop:quit 0))
                 (error (c)
                   (format *error-output* "ERROR ~a~%" c)
                   (uiop:quit 1))))
   :save-runtime-options t))

(handler-case (build)
  (error (c)
    (format *error-output* "ERROR ~a~%" c)
    (uiop:quit 1))
  (:no-error (c)
    (declare (ignore c))
    (uiop:quit 0)))
