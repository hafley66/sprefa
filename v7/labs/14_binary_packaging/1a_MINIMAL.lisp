(defpackage #:sprefa-lab-14-minimal
  (:use #:cl)
  (:export #:main #:run-probe))

(in-package #:sprefa-lab-14-minimal)

(defun run-probe ()
  (format t "PROBE shape=minimal-sbcl~%")
  (finish-output))

(defun main ()
  (handler-case
      (progn
        (run-probe)
        (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))
