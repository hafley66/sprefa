;;;; Two SBCL image entry points for the binary-packaging lab.
;;;;
;;;; 6_BUILD.lisp saves one of these functions to an external output path.

(defpackage #:binary-packaging-lab
  (:use #:cl)
  (:export #:main-minimal #:main-swi-subprocess))

(in-package #:binary-packaging-lab)

(defparameter +swi-query-goal+
  "assertz(edge(a,b)),assertz(edge(b,c)),assertz(edge(c,a)),assertz(edge(c,d)),assertz((path(X,Y):-edge(X,Y))),assertz((path(X,Y):-edge(X,Z),path(Z,Y))),table(path/2),call_with_time_limit(1,(setof(Y,path(a,Y),Ys),format('QUERY ~q~n',[Ys]))),halt")

(defun run-swi-query ()
  (uiop:run-program
   (list "swipl" "-q" "-g" +swi-query-goal+)
   :output :string
   :error-output :string))

(defun exit-on-error (thunk)
  (handler-case
      (progn
        (funcall thunk)
        (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))

(defun main-minimal ()
  (exit-on-error
   (lambda ()
     (format t "SBCL-MINIMAL~%"))))

(defun main-swi-subprocess ()
  (exit-on-error
   (lambda ()
     (let ((output (run-swi-query)))
       (unless (string= output (format nil "QUERY [a,b,c,d]~%"))
         (error "unexpected SWI output: ~S" output))
       (write-string output)))))
