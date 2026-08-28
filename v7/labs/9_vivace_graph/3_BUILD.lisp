;;;; Build a provenance-checked VivaceGraph SBCL executable image.
;;;; Required: VIVACE_SRC, QL_SETUP, VIVACE_OUT.

(require :asdf)

(defparameter *lab-directory*
  (uiop:pathname-directory-pathname (or *load-truename* (uiop:argv0))))
(defparameter *output*
  (or (uiop:getenv "VIVACE_OUT")
      (error "set VIVACE_OUT to an external executable path")))

(setf (uiop:getenv "VIVACE_NO_RUN") "1")
(load (merge-pathnames "2_PROBE.lisp" *lab-directory*))

(funcall (symbol-function (find-symbol "VERIFY-PIN" "VIVACE-GRAPH-LAB-BOOTSTRAP")))
(setf (symbol-value (find-symbol "*PINNED-IMAGE-LIBRARY*" "VIVACE-GRAPH-LAB-BOOTSTRAP"))
      (symbol-value (find-symbol "*PIN*" "VIVACE-GRAPH-LAB-BOOTSTRAP")))

(sb-ext:save-lisp-and-die
 *output*
 :executable t
 :toplevel (lambda ()
             (let ((*package* (find-package :vivace-graph-lab)))
               (vivace-graph-lab:main)))
 :save-runtime-options t)
