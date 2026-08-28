;;;; Build a standalone SBCL image containing Screamer and the probe.
;;;; Required: SCREAMER_SRC=<pinned checkout>
;;;; Optional: SCREAMER_LAB_OUTPUT=<external executable path>

(require :asdf)

(setf (uiop:getenv "SCREAMER_LAB_NO_RUN") "1")
(load (merge-pathnames "2_PROBE.lisp" *load-truename*))

(sb-ext:save-lisp-and-die
 (or (uiop:getenv "SCREAMER_LAB_OUTPUT")
     "/private/tmp/sprefa-v7-screamer-lab-ce50614-20260828")
 :executable t
 :toplevel #'(lambda ()
               (handler-case
                   (let ((*package* (find-package :screamer-lab)))
                     (screamer-lab:run-probe)
                     (uiop:quit 0))
                 (error (condition)
                   (format *error-output* "ERROR ~a~%" condition)
                   (uiop:quit 1))))
 :save-runtime-options t)
