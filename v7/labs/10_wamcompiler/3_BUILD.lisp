;;; Build a saved SBCL image at the external WAMCOMPILER_OUT pathname.

(require :asdf)

(defpackage #:wamcompiler-lab-build
  (:use #:cl))

(in-package #:wamcompiler-lab-build)

(defparameter *lab-dir*
  (make-pathname :defaults (or *load-truename* (uiop:argv0)) :name nil :type nil))
(defparameter *out*
  (or (uiop:getenv "WAMCOMPILER_OUT")
      (error "set WAMCOMPILER_OUT to an external output path")))

(defun build ()
  (setf (uiop:getenv "PROBE_NOEXEC") "1")
  (load (merge-pathnames "2_PROBE.lisp" *lab-dir*))
  (funcall (symbol-function (find-symbol "VERIFY-PIN" "WAMCOMPILER-LAB-PROBE")))
  (funcall (symbol-function (find-symbol "LOAD-LIBRARY" "WAMCOMPILER-LAB-PROBE")))
  (setf (symbol-value (find-symbol "*PINNED-IMAGE-LIBRARY*" "WAMCOMPILER-LAB-PROBE"))
        (symbol-value (find-symbol "*PIN*" "WAMCOMPILER-LAB-PROBE")))
  (setf (symbol-value (find-symbol "*SAVED-IMAGE-PATH*" "WAMCOMPILER-LAB-PROBE"))
        *out*)
  (sb-ext:save-lisp-and-die
   *out* :executable t :save-runtime-options t
   :toplevel (lambda ()
               (handler-case
                   (progn
                     (funcall (symbol-function (find-symbol "MAIN" "WAMCOMPILER-LAB-PROBE")))
                     (uiop:quit 0))
                 (error (c)
                   (format *error-output* "ERROR ~a~%" c)
                   (uiop:quit 1))))))

(handler-case
    (progn (build) (uiop:quit 0))
  (error (c)
    (format *error-output* "ERROR ~a~%" c)
    (uiop:quit 1)))
