;;;; Build a standalone SBCL image containing grph and the probe.
;;;; Optional: CL_GRPH_LAB_OUTPUT=<external executable path>

(require :asdf)

(pushnew :cl-grph-lab-build *features*)
(load (merge-pathnames "2_PROBE.lisp" *load-truename*))

(sb-ext:save-lisp-and-die
 (or (uiop:getenv "CL_GRPH_LAB_OUTPUT")
     "/private/tmp/sprefa-v7-cl-grph-lab-d9d5edd-20260828")
 :executable t
 :toplevel #'(lambda ()
               (handler-case
                   (let ((*package* (find-package :cl-grph-lab)))
                     (cl-grph-lab:run)
                     (uiop:quit 0))
                 (error (condition)
                   (format *error-output* "ERROR ~a~%" condition)
                   (uiop:quit 1))))
 :save-runtime-options t)
