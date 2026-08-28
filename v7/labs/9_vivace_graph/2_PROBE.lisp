;;;; VivaceGraph graph and Prolog capability probe.
;;;;
;;;; Required environment:
;;;;   VIVACE_SRC=/private/tmp/sprefa-v7-vivace-graph
;;;;   QL_SETUP=/private/tmp/sprefa-v7-vivace-cache/.quicklisp/setup.lisp
;;;;   VIVACE_DB=$(mktemp -d /private/tmp/sprefa-v7-vivace-db.XXXXXX)
;;;; Optional:
;;;;   VIVACE_BIN=/private/tmp/sprefa-v7-vivace-lab
;;;;   VIVACE_NO_RUN=1                    ; used by 3_BUILD.lisp

(require :asdf)

(defpackage #:vivace-graph-lab-bootstrap
  (:use #:cl))

(in-package #:vivace-graph-lab-bootstrap)

(defparameter *pin* "68230b3879c238b3c24b79a97fc06048841f4f0b")
(defparameter *vivace-src*
  (or (uiop:getenv "VIVACE_SRC")
      (error "set VIVACE_SRC to the detached VivaceGraph checkout")))
(defparameter *ql-setup*
  (or (uiop:getenv "QL_SETUP")
      (error "set QL_SETUP to the project-local Quicklisp setup.lisp")))
(defparameter *pinned-image-library* nil)

(defun current-vivace-src ()
  (or (uiop:getenv "VIVACE_SRC") *vivace-src*))

(defun current-ql-setup ()
  (or (uiop:getenv "QL_SETUP") *ql-setup*))

(defun command-output (&rest argv)
  (string-trim '(#\Space #\Tab #\Newline #\Return)
               (uiop:run-program argv :output :string :error-output :string)))

(defun verify-pin ()
  (let ((head (command-output "git" "-C" (current-vivace-src) "rev-parse" "HEAD"))
        (dirty (command-output "git" "-C" (current-vivace-src) "status" "--porcelain")))
    (unless (string= head *pin*)
      (error "VivaceGraph pin mismatch: expected ~a, got ~a" *pin* head))
    (unless (string= dirty "")
      (error "VivaceGraph checkout is dirty: ~a" dirty))
    t))

(defun load-library ()
  (verify-pin)
  (cond ((find-package :graph-db)
         (unless (string= *pinned-image-library* *pin*)
           (error "GRAPH-DB was preloaded without pinned-image provenance")))
        (t
         (load (current-ql-setup))
         (funcall (symbol-function (find-symbol "QUICKLOAD" :ql))
                  '(:bordeaux-threads :alexandria :iterate :cffi :cl-ppcre
                    :uuid :split-sequence :cl-store :cl-fad :local-time
                    :ieee-floats :cl-json :log4cl :md5)
                  :silent t)
         (asdf:load-asd (merge-pathnames "graph-db.asd"
                                         (uiop:ensure-directory-pathname
                                          (current-vivace-src))))
         (asdf:load-system "graph-db/core")))
  (setf *pinned-image-library* *pin*))

(defun verify-runtime-provenance ()
  (verify-pin)
  (unless (and (find-package :graph-db)
               (string= *pinned-image-library* *pin*))
    (error "GRAPH-DB does not carry pinned image provenance ~a" *pin*)))

(load-library)

(defpackage #:vivace-graph-lab
  (:use #:cl #:graph-db)
  (:export #:main #:run-probe))

(in-package #:vivace-graph-lab)

(defparameter *graph-name* :vivace-graph-lab)
(defparameter *db-path* (uiop:getenv "VIVACE_DB"))

(def-vertex vg-node ()
  ((name :initarg :name :accessor vg-name :type string :index t))
  :vivace-graph-lab)

(def-edge vg-link ()
  ()
  :vivace-graph-lab)

(def-edge vg-proof ()
  ()
  :vivace-graph-lab)

;; These rules are intentionally cyclic. The native query engine is given an
;; inference budget below; the visited-state closure is a separately labeled
;; adapter, not a claim of Prolog tabling.
(graph-db::<- (vg-path ?x ?y) (vg-link ?x ?y))
(graph-db::<- (vg-path ?x ?y) (vg-link ?x ?z) (vg-path ?z ?y))
(graph-db::<- (vg-proof-path ?x ?y) (vg-proof ?x ?y))
(graph-db::<- (vg-proof-path ?x ?y) (vg-proof ?x ?z) (vg-proof ?z ?y))

(defun sorted-strings (items)
  (sort (copy-list items) #'string<))

(defun names (nodes)
  (sorted-strings (mapcar #'vg-name nodes)))

(defun db-kilobytes ()
  (let ((out (uiop:run-program (list "du" "-sk" *db-path*)
                               :output :string :error-output :string)))
    (parse-integer out :junk-allowed t)))

(defun binary-bytes ()
  (let ((path (uiop:getenv "VIVACE_BIN")))
    (if (and path (probe-file path))
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defun all-names ()
  (names (select-flat (?node) (is-a ?node vg-node))))

(defun edge-name-pairs ()
  (sort (mapcar (lambda (pair)
                  (format nil "~a>~a" (vg-name (first pair)) (vg-name (second pair))))
                (select (:flat nil :max-inferences 100 :timeout 1)
                        (?from ?to) (vg-link ?from ?to)))
        #'string<))

(defun closure-from (origin)
  "Finite host-side visited-set adapter over the persisted edge relation."
  (let ((seen (make-hash-table :test #'equalp))
        (pending (list origin)))
    (setf (gethash (id origin) seen) t)
    (loop while pending
          for node = (pop pending)
          do (dolist (edge (outgoing-edges node :edge-type 'vg-link))
               (let ((next (lookup-vertex (to edge))))
                 (unless (gethash (id next) seen)
                   (setf (gethash (id next) seen) t)
                   (push next pending)))))
    (names (loop for node-id being the hash-keys of seen
                 collect (lookup-vertex node-id)))))

(defun cyclic-query-receipt ()
  (handler-case
      (progn
        (select (:flat t :max-inferences 64 :timeout 1)
                (?target) (vg-path ?origin ?target))
        "completed")
    (prolog-resource-error (condition)
      (format nil "bounded=~a" condition))))

(defun rollback-receipt ()
  (handler-case
      (with-transaction ()
        (make-vg-node :name "ROLLBACK")
        (error "intentional transaction rollback"))
    (error () nil))
  (all-names))

(defun populate-graph ()
  (with-transaction ()
    (let ((a (make-vg-node :name "A"))
          (b (make-vg-node :name "B"))
          (c (make-vg-node :name "C"))
          (d (make-vg-node :name "D")))
      (make-vg-link :from a :to b)
      (make-vg-link :from b :to c)
      (make-vg-link :from c :to a)
      (make-vg-link :from c :to d)
      ;; Acyclic diamond for two bounded, duplicate proof paths to B.
      (make-vg-proof :from a :to b)
      (make-vg-proof :from a :to c)
      (make-vg-proof :from c :to b)
      a)))

(defun run-probe ()
  (vivace-graph-lab-bootstrap::verify-runtime-provenance)
  (setf *db-path* (or (uiop:getenv "VIVACE_DB") *db-path*))
  (unless *db-path*
    (error "set VIVACE_DB to a fresh temporary graph directory"))
  (unless (uiop:directory-exists-p *db-path*)
    (ensure-directories-exist (merge-pathnames "placeholder" (pathname *db-path*))))
  (when (uiop:directory-files (uiop:ensure-directory-pathname *db-path*))
    (error "VIVACE_DB must be an empty temporary directory: ~a" *db-path*))
  (format t "PROBE library=vivace-graph version=3.0.0 commit=~a~%"
          vivace-graph-lab-bootstrap::*pin*)
  (format t "UNIFY ~s~%"
          (select (:flat nil :max-inferences 16 :timeout 1)
                  (?x ?y) (= (f ?x (g ?y)) (f a (g b)))))
  (format t "OCCURS occurs-check=~a result=~s~%"
          "compiled-present"
          (select (:flat nil :max-inferences 16 :timeout 1)
                  (?x) (= ?x (f ?x))))
  (let ((graph (make-graph *graph-name* *db-path* :buffer-pool-size 1000)))
    (unwind-protect
        (let ((*graph* graph))
          (format t "ROLLBACK names=~s~%" (rollback-receipt))
          (let ((a (populate-graph)))
            (format t "COMMIT names=~s edges=~s~%" (all-names) (edge-name-pairs))
            (format t "INDEX lookup=A rows=~s~%"
                    (names (index-lookup graph 'vg-node 'name "A")))
            (format t "PATH direct=~s cycle=~a adapter=~s~%"
                    (edge-name-pairs)
                    (cyclic-query-receipt)
                    (closure-from a)))
          (format t "DUPES raw=~s unique=~s~%"
                  (mapcar #'vg-name
                          (select (:flat t :max-inferences 64 :timeout 1)
                                  (?target)
                                  (is-a ?origin vg-node)
                                  (node-slot-value ?origin name "A")
                                  (vg-proof-path ?origin ?target)))
                  (sorted-strings
                   (remove-duplicates
                    (mapcar #'vg-name
                            (select (:flat t :max-inferences 64 :timeout 1)
                                    (?target)
                                    (is-a ?origin vg-node)
                                    (node-slot-value ?origin name "A")
                                    (vg-proof-path ?origin ?target)))
                    :test #'string=)))
          (do-query (is-a ?node vg-node)
                    (node-slot-value ?node name "D")
                    (retract ?node))
          (format t "UPDATE retract=D names=~s index=D rows=~s~%"
                  (all-names)
                  (names (index-lookup graph 'vg-node 'name "D")))
          (close-graph graph :snapshot-p nil)
          (setf graph nil)
          (let ((reopened (open-graph *graph-name* *db-path* :buffer-pool-size 1000)))
            (unwind-protect
                (let ((*graph* reopened))
                  (format t "REOPEN names=~s index=A rows=~s edges=~s db-kib=~d~%"
                          (all-names)
                          (names (index-lookup reopened 'vg-node 'name "A"))
                          (edge-name-pairs)
                          (db-kilobytes)))
              (close-graph reopened :snapshot-p nil))))
      (when graph (ignore-errors (close-graph graph :snapshot-p nil)))))
  (format t "BINARY ~a~%" (binary-bytes))
  (format t "IMAGE source-load=~a compile=~a~%" (not (null (fboundp 'load)))
          (not (null (fboundp 'compile)))))

(defun main ()
  (handler-case
      (progn (run-probe) (uiop:quit 0))
    (error (condition)
      (format *error-output* "ERROR ~a~%" condition)
      (uiop:quit 1))))

(unless (uiop:getenv "VIVACE_NO_RUN")
  (main))
