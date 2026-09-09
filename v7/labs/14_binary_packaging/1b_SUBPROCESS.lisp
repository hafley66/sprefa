(defpackage #:sprefa-lab-14-subprocess
  (:use #:cl)
  (:export #:main #:run-probe #:run-benchmark))

(in-package #:sprefa-lab-14-subprocess)

(defparameter +swi-query+
  "once((member(X,[a,b,c,d]),X=d)),write_canonical(X),nl")

(defun run-swi-query ()
  (string-trim '(#\Space #\Tab #\Newline #\Return)
               (uiop:run-program
                (list "swipl" "--quiet"
                      "-g" +swi-query+ "-t" "halt")
                :output :string
                :error-output :string)))

(defun run-benchmark ()
  (let* ((count 20)
         (start (get-internal-real-time))
         (answers (loop repeat count collect (run-swi-query)))
         (seconds (/ (- (get-internal-real-time) start)
                     internal-time-units-per-second)))
    (unless (every (lambda (answer) (string= answer "d")) answers)
      (error "Unexpected SWI answer set ~S" answers))
    (format t "BENCHMARK shape=sbcl-subprocess-swi count=~D seconds=~,6F ms-per-query=~,3F~%"
            count seconds (* 1000 (/ seconds count)))
    (finish-output)))

(defun run-probe ()
  (format t "PROBE shape=sbcl-subprocess-swi answer=~A~%" (run-swi-query))
  (finish-output))

(defun main ()
  (handler-case
      (progn
        (if (uiop:getenv "LAB14_BENCH")
            (run-benchmark)
            (run-probe))
        (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))
