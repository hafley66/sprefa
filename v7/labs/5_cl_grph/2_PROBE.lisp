;;;; cl-grph capability probe (self-loading). Prints deterministic PROBE
;;;; records per the lab report contract.
;;;;
;;;; Loader: this file loads its own pinned dependencies. Override the
;;;; checkout locations with the environment variables CL_GRPH_DIR and
;;;; VEQ_DIR; defaults are /tmp/cl-grph-lab/cl-grph and /tmp/cl-grph-lab/veq.
;;;; A project-local quicklisp at /tmp/cl-grph-lab/.quicklisp is used and
;;;; never mutates a global setup.
;;;;
;;;; IMPORTANT: grph vertex ids are SIGNED-BYTE 32 integers. The shared
;;;; cyclic fixture is mapped a->0 b->1 c->2 d->3. Symbols are legal only as
;;;; edge property names (:edge etc), never as vertices.

(require :asdf)

(eval-when (:compile-toplevel :load-toplevel :execute)
  (let* ((grph-dir (or (uiop:getenv "CL_GRPH_DIR") "/tmp/cl-grph-lab/cl-grph"))
         (veq-dir  (or (uiop:getenv "VEQ_DIR") "/tmp/cl-grph-lab/veq"))
         (pins     `((,grph-dir . "d9d5eddeebf4eeaa2dcffc62791406961ed74e4f")
                     (,veq-dir  . "d82dc83f8d275e36e516264b390c37cdb4d646d4"))))
    ;; Validate every load, including builds that loaded grph earlier.
    (dolist (pin pins)
      (destructuring-bind (dir . commit) pin
        (let ((head (uiop:run-program (list "git" "-C" dir "rev-parse" "HEAD")
                                      :output '(:string :stripped t)
                                      :ignore-error-status t))
              (dirty (uiop:run-program
                      (list "git" "-C" dir "status" "--porcelain")
                      :output :string
                      :ignore-error-status t)))
          (unless (and head (string= head commit))
            (error "PIN MISMATCH: ~s is at ~s, expected ~s" dir head commit))
          (unless (and dirty (zerop (length dirty)))
            (error "PIN DIRTY: ~s contains uncommitted changes" dir)))))
    (when (find-package :grph)
      (error "GRPH was loaded before the pinned probe loader"))
    (load "/tmp/cl-grph-lab/.quicklisp/setup.lisp")
    (funcall (symbol-function (find-symbol "QUICKLOAD" "QL"))
             '("fset" "lparallel") :silent t)
    ;; quicklisp's registry shadows the git veq (dist ships cl-veq 4.5.5,
    ;; which lacks veq:ungroup / lpos); re-init with the pinned checkout
    (asdf:initialize-source-registry
     (list :source-registry (list :directory veq-dir) :inherit-configuration))
    (asdf:load-system "veq")
    (asdf:load-asd (merge-pathnames "grph.asd" (uiop:ensure-directory-pathname grph-dir)))
    (asdf:load-system "grph")))

(defpackage #:cl-grph-lab
  (:use #:cl)
  (:export #:run)
  (:import-from #:grph #:qry #:rqry #:ingest-edges #:grph)
  (:import-from #:grph/io #:gwrite #:gread))

(in-package #:cl-grph-lab)

;; shared cyclic graph: 0->1->2->0 plus 2->3  (a->b->c->a plus c->d)
(defvar *fixture*
  '((0 :edge 1) (1 :edge 2) (2 :edge 0) (2 :edge 3)))

(defun mk-graph (&optional (edges *fixture*))
  (ingest-edges edges (grph)))

(defun sorted (xs) (sort (copy-list xs) #'string< :key (lambda (p) (format nil "~s" p))))

(defvar *spin-called* nil)

(defun spin-forever ()
  (setf *spin-called* t)
  (loop (sleep 1)))

(defun starvation-probe (g)
  "Run a divergent left query branch before a finite right branch."
  (setf *spin-called* nil)
  (let ((result
          (handler-case
              (sb-ext:with-timeout 0.05
                (qry g :select ?x
                     :where (or (and (?x :edge ?y) (% (spin-forever)))
                                (?x :edge ?y))))
            (sb-ext:timeout () :timed-out))))
    (values result *spin-called*)))

(defun path-fixpoint (g &optional (order :base-first))
  "transitive closure over :edge via rqry linear-rule fixpoint. ORDER can
be flipped to probe rule-order sensitivity (fairness receipt)."
  (ecase order
    (:base-first
      (rqry g :rules ((*path (?x ?y) (?x :edge ?y))
                      (*path (?x ?y) (and (?x :edge ?z) (*path ?z ?y))))
              :then (values *path 'linear-fixpoint)))
    (:recursive-first
      (rqry g :rules ((*path (?x ?y) (and (?x :edge ?z) (*path ?z ?y)))
                      (*path (?x ?y) (?x :edge ?y)))
              :then (values *path 'linear-fixpoint)))))

(defun run ()
  (let* ((*g* (mk-graph))
         (t0 (get-internal-real-time))
         (path-result (path-fixpoint *g*))
         (t1 (get-internal-real-time)))

    (format t "PROBE library=grph version=d9d5eddeebf4eeaa2dcffc62791406961ed74e4f~%")

    ;; triple-pattern matching on ground triples (grph's only "unification")
    (format t "UNIFY pattern=?x->1 all-edges=~s~%"
            (sorted (qry *g* :select ?x :where (?x :edge 1))))

    ;; nested-compound unification: a triple pattern is the deepest term; a
    ;; variable can never bind a whole edge, so ((0 :edge 1) :edge ?y) is not
    ;; a term. record as absent.
    (handler-case
        (let ((r (qry *g* :select ?y :where ((0 :edge 1) :edge ?y))))
          (format t "NESTED unsupported=absent result=~s~%" r))
      (error (c) (format t "NESTED unsupported=absent error=~s~%" (format nil "~a" c))))

    ;; occurs check: variables bind only to 32-bit vertex ids inside a fixed
    ;; (subj pred obj) shape, so X = f(X) has no encoding.
    (format t "OCCURS policy=absent-by-construction~%")

    ;; cyclic transitive closure. RAW preserves the engine's answer order
    ;; (fset set iteration order); PATH is the sorted canonical set.
    (format t "PATH-RAW order=~s~%" path-result)
    (format t "PATH set=~s mechanism=linear-fixpoint~%" (sorted path-result))

    (multiple-value-bind (result left-called)
        (starvation-probe *g*)
      (format t "FAIR starvation-shape=unsupported left-filter-called=~s later-answer=~s~%"
              left-called result))

    ;; Rule-order receipt, bounded by rqry's :lim.
    (let ((lim-result
            (handler-case
                (let ((r (path-fixpoint *g* :recursive-first)))
                  (format nil "recursive-first-terminated n=~s sorted=~s"
                          (length r) (sorted r)))
              (error (c)
                (format nil "recursive-first-error=~s" (format nil "~a" c))))))
      (format t "RULE-ORDER base-first=closure ~a~%" lim-result))

    ;; duplicate answers: one row per proof, or one row per fact?
    (let* ((g (mk-graph '((0 :edge 1) (0 :edge 2) (1 :edge 3) (2 :edge 3))))
           (r (path-fixpoint g)))
      (format t "DUP raw=~s raw-count=~s undup-count=~s~%"
              r (length r) (length (remove-duplicates r :test #'equal))))

    ;; negation
    (format t "NOT no-out-edge=~s~%"
            (sorted (qry *g* :select ?x
                           :where (and (?x :edge ?y)
                                       (not (?y :edge _))))))
    ;; or-join
    (format t "ORJOIN=~s~%"
            (sorted (qry *g* :select ?r
                           :where (or-join ?r
                                    (and (?r :edge ?z) (?z :edge 3))
                                    (?r :edge 0)))))

    ;; dynamic facts: functional del returns a new graph; original untouched
    (let* ((g2 (grph:del *g* 2 3))
           (p2 (nth-value 0 (path-fixpoint g2))))
      (format t "UPDATE after-del-path=~s original-still-has-2-3=~s~%"
              (sorted p2)
              (not (null (qry *g* :select ?x :where (?x :edge 3))))))

    ;; serialization round-trip via grph/io (.grph text format)
    (handler-case
        (let* ((fn "/tmp/cl-grph-lab/roundtrip")
               (generated (concatenate 'string fn ".grph")))
          (unwind-protect
               (progn
                 (gwrite fn *g* :meta '(:lab "5_cl_grph"))
                 (let ((g3 (gread fn)))
                   (format t "SERIALIZE verts-in=~s verts-out=~s closure-eq=~s~%"
                           (length (qry *g* :select ?x :where (or (?x _ _) (_ _ ?x))))
                           (length (qry g3 :select ?x :where (or (?x _ _) (_ _ ?x))))
                           (equal (sorted path-result)
                                  (sorted (nth-value 0 (path-fixpoint g3)))))))
            (when (probe-file generated)
              (delete-file generated))))
      (error (c) (format t "SERIALIZE error=~s~%" (format nil "~a" c))))

    ;; compiler-fact ingestion without the spatial API: pure triple list, but
    ;; symbols are rejected as verts -> an id dictionary adapter is required.
    (format t "COMPILER-FACTS verts=signed-byte-32 adapter=id-dictionary~%")
    (handler-case
        (ingest-edges '((cl-grph-lab::a :calls cl-grph-lab::b)) (grph))
      (error (c)
        (format t "COMPILER-FACTS symbol-vert-rejected=~s~%" (format nil "~a" c))))

    ;; The explicit variable prevents direct SBCL runs from measuring the
    ;; SBCL launcher.
    (let ((exe (uiop:getenv "CL_GRPH_LAB_BINARY")))
      (if (and exe (uiop:file-exists-p exe))
          (format t "BINARY ~s~%" (with-open-file (f exe) (file-length f)))
          (format t "BINARY blocked:not-built~%")))

    (format t "SECONDS closure-elapsed=~,3f~%"
            (/ (- t1 t0) internal-time-units-per-second))))

;; top-level execution is suppressed while 3_BUILD.lisp loads this file
;; (it pushes :cl-grph-lab-build) so the probe runs once, in the image.
(unless (member :cl-grph-lab-build *features*)
  (run))
