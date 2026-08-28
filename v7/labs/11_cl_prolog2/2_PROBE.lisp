(defpackage #:cl-prolog2-lab-11
  (:use #:cl)
  (:import-from #:cl-prolog2
                #:print-sexp
                #:run-prolog)
  (:export #:main
           #:run-probe
           #:run-bridge-benchmark
           #:*binary-path*))

(in-package #:cl-prolog2-lab-11)

(defvar *binary-path* nil)

(defparameter +shared-fixture+
  `((:- (dynamic (/ edge 2)))
    (:- (table (/ path 2)))
    (edge a b)
    (edge b c)
    (edge c a)
    (edge c d)
    (:- (path ?left ?right)
        (edge ?left ?right))
    (:- (path ?left ?right)
        (edge ?left ?middle)
        (path ?middle ?right))
    (:- main
        (write "UNIFY ")
        (= ?term (f (g a)))
        (write_canonical ?term)
        nl
        (write "OCCURS standard-rational-tree=")
        (= ?cycle (f ?cycle))
        (write "success ")
        (write "occurs-check=")
        (or (unify_with_occurs_check ?fresh (f ?fresh))
            (write "fail"))
        nl
        (setof ?destination (path a ?destination) ?destinations)
        (write "PATH ")
        (print-sexp ?destinations)
        nl
        (findall ?answer (path a ?answer) ?answers)
        (sort ?answers ?ordered-answers)
        (write "ANSWERS ")
        (print-sexp ?ordered-answers)
        nl
        (not (path a z))
        (write "NEGATIVE true")
        nl
        (write "FAIR absent-from-probe")
        nl
        (catch (throw bridge-probe)
               ?exception
               (and (write "EXCEPTION ")
                    (print-sexp ?exception)
                    nl))
        (retract (edge c d))
        abolish_all_tables
        (setof ?updated (path a ?updated) ?updates)
        (write "UPDATE ")
        (print-sexp ?updates)
        nl
        halt)
    (:- (initialization main))
    ,@(print-sexp :swi t)))

(defparameter +benchmark-fixture+
  '((:- main
       (write "ok")
       halt)
    (:- (initialization main))))

(defun binary-record ()
  (if (and *binary-path* (probe-file *binary-path*))
      (format nil "BINARY ~D" (with-open-file (stream *binary-path*
                                                       :element-type '(unsigned-byte 8))
                                (file-length stream)))
      "BINARY source-loaded"))

(defun run-probe ()
  (format t "PROBE library=cl-prolog2 version=0.1 backend=swi swipl=10.0.2~%")
  (write-string (run-prolog +shared-fixture+ :swi :debug 2))
  (format t "~A~%" (binary-record)))

(defun run-bridge-benchmark (&optional (iterations 20))
  (let ((started (get-internal-real-time)))
    (loop repeat iterations
          do
      (run-prolog +benchmark-fixture+ :swi))
    (/ (- (get-internal-real-time) started)
       internal-time-units-per-second)))

(defun main ()
  (handler-case
      (progn
        (run-probe)
        (uiop:quit 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (uiop:quit 1))))
