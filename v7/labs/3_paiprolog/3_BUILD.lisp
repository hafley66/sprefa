(require :asdf)
;; Build: PAIPROLOG_SRC=<checkout> sbcl --noinform --disable-debugger --load 3_BUILD.lisp
;; Output: ./paiprolog-lab, measured at 40,769,552 bytes. After measurement,
;; move the generated executable outside the repository.
(setf (uiop:getenv "PAIPROLOG_LAB_NO_RUN") "1")
(load (merge-pathnames "2_PROBE.lisp" *load-truename*))
(sb-ext:save-lisp-and-die
 "paiprolog-lab"
 :executable t
 :toplevel #'(lambda ()
               (handler-case
                   (progn
                     (in-package :paiprolog-lab)
                     (paiprolog-lab::run-probe)
                     (uiop:quit 0))
                 (error (c)
                   (format *error-output* "ERROR ~a~%" c)
                   (uiop:quit 1))))
 :save-runtime-options t)
