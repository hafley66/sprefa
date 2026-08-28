;;;; Screamer capability probe.
;;;; Required: SCREAMER_SRC=<pinned checkout>
;;;; Optional: SCREAMER_LAB_BINARY=<saved executable path>

(require :asdf)

(eval-when (:compile-toplevel :load-toplevel :execute)
  (let* ((src (uiop:getenv "SCREAMER_SRC"))
         (pinned "ce50614024de090b376107668da5e53232540ec7"))
    (unless src
      (error "SCREAMER_SRC is required"))
    (let ((head
            (string-trim
             '(#\Space #\Tab #\Newline #\Return)
             (uiop:run-program
              (list "git" "-C" src "rev-parse" "HEAD")
              :output :string)))
          (dirty
            (uiop:run-program
             (list "git" "-C" src "status" "--porcelain")
             :output :string)))
      (unless (string= head pinned)
        (error "SCREAMER_SRC commit ~a does not match ~a" head pinned))
      (unless (zerop (length dirty))
        (error "SCREAMER_SRC contains uncommitted changes")))
    (asdf:load-asd
     (merge-pathnames "screamer.asd"
                      (uiop:ensure-directory-pathname src)))
    (asdf:load-system "screamer")))

(screamer:define-screamer-package #:screamer-lab
  (:export #:run-probe))

(in-package #:screamer-lab)

(defparameter *pinned-commit*
  "ce50614024de090b376107668da5e53232540ec7")

;;; First-order term adapter

(defstruct (logic-variable (:constructor make-logic-variable (id)))
  id)

(defvar *next-logic-variable-id* 0)

(defun fresh-variable ()
  (prog1 (make-logic-variable *next-logic-variable-id*)
    (incf *next-logic-variable-id*)))

(defun walk-term (term substitution)
  (if (logic-variable-p term)
      (let ((binding (assoc term substitution :test #'eq)))
        (if binding
            (walk-term (cdr binding) substitution)
            term))
      term))

(defun occurs-in-term-p (variable term substitution)
  (let ((term (walk-term term substitution)))
    (cond
      ((logic-variable-p term) (eq variable term))
      ((consp term)
       (or (occurs-in-term-p variable (car term) substitution)
           (occurs-in-term-p variable (cdr term) substitution)))
      (t nil))))

(defun bind-variable (variable term substitution)
  (let ((term (walk-term term substitution)))
    (cond
      ((eq variable term) (values substitution t))
      ((occurs-in-term-p variable term substitution) (values nil nil))
      (t (values (acons variable term substitution) t)))))

(defun unify-terms (left right substitution)
  (let ((left (walk-term left substitution))
        (right (walk-term right substitution)))
    (cond
      ((eq left right) (values substitution t))
      ((logic-variable-p left)
       (bind-variable left right substitution))
      ((logic-variable-p right)
       (bind-variable right left substitution))
      ((and (consp left) (consp right))
       (multiple-value-bind (next ok)
           (unify-terms (car left) (car right) substitution)
         (if ok
             (unify-terms (cdr left) (cdr right) next)
             (values nil nil))))
      ((equal left right) (values substitution t))
      (t (values nil nil)))))

(defun reify-term (term substitution)
  (let ((term (walk-term term substitution)))
    (cond
      ((logic-variable-p term)
       (format nil "_~d" (logic-variable-id term)))
      ((consp term)
       (cons (reify-term (car term) substitution)
             (reify-term (cdr term) substitution)))
      (t term))))

;;; Nondeterministic relation adapters

(defparameter *edges* nil)

(defun edge-target (start)
  (let ((edge (a-member-of *edges*)))
    (unless (eql start (first edge))
      (fail))
    (second edge)))

(defun path-end (start remaining-depth)
  (unless (plusp remaining-depth)
    (fail))
  (let ((next (edge-target start)))
    (either next
            (path-end next (1- remaining-depth)))))

(defun append-split (items)
  (either
   (list nil items)
   (if (consp items)
       (let ((split (append-split (cdr items))))
         (list (cons (car items) (first split))
               (second split)))
       (fail))))

(defun node-without-outgoing-edge ()
  (let ((node (a-member-of '(a b c d))))
    (when (find node *edges* :key #'first :test #'eql)
      (fail))
    node))

(defun sorted-symbols (values)
  (sort (copy-list values) #'string< :key #'symbol-name))

(defun canonical-symbol-set (values)
  (sorted-symbols (remove-duplicates values :test #'eql)))

;;; Bounded starvation probe

(cl:defun spin-forever ()
  (loop (sleep 1)))

(defun collect-starving-choice ()
  (all-values (either (spin-forever) :reachable)))

(cl:defun bounded-starvation-probe ()
  (handler-case
      (sb-ext:with-timeout 0.05
        (collect-starving-choice))
    (sb-ext:timeout () :timed-out)))

(defun binary-bytes ()
  (let ((path (uiop:getenv "SCREAMER_LAB_BINARY")))
    (if (and path (probe-file path))
        (with-open-file (stream path :element-type '(unsigned-byte 8))
          (file-length stream))
        "blocked:not-built")))

(defun run-probe ()
  (format t "PROBE library=screamer version=~a commit=~a~%"
          screamer:*screamer-version* *pinned-commit*)

  (let ((raw (all-values (either 'first 'second 'third))))
    (format t "ORDER raw=~s~%" raw))

  (let ((x (fresh-variable))
        (y (fresh-variable)))
    (multiple-value-bind (substitution ok)
        (unify-terms (list 'f x (list 'g 'a))
                     (list 'f 'b (list 'g y))
                     nil)
      (format t "UNIFY ok=~s values=~s~%"
              ok
              (and ok
                   (list (reify-term x substitution)
                         (reify-term y substitution))))))

  (let ((x (fresh-variable)))
    (multiple-value-bind (substitution ok)
        (unify-terms x (list 'f x) nil)
      (declare (ignore substitution))
      (format t "OCCURS policy=adapter-on cyclic-unify=~s~%"
              (if ok 'succeeds 'fails))))

  (let ((splits (all-values (append-split '(a b c)))))
    (format t "APPEND forward=~s backward=~s mechanism=nondeterministic-adapter~%"
            (append '(a b) '(c))
            splits))

  (setf *edges* '((a b) (b c) (c a) (c d)))
  (let* ((raw (all-values (path-end 'a 4)))
         (canonical (canonical-symbol-set raw)))
    (format t "PATH raw=~s sorted=~s mechanism=depth-bound~%"
            raw canonical))

  (let ((raw (all-values (node-without-outgoing-edge))))
    (format t "NEGATION raw=~s sorted=~s mechanism=finite-closed-world-adapter~%"
            raw (canonical-symbol-set raw)))

  (setf *edges* '((a b) (b c) (c a)))
  (format t "UPDATE sorted=~s mechanism=replace-fact-list~%"
          (canonical-symbol-set (all-values (path-end 'a 4))))

  (format t "FAIR left-branch=~(~a~) later-answer=reachable~%"
          (bounded-starvation-probe))

  (let ((answers
          (let ((x (an-integer-betweenv 1 4))
                (y (an-integer-betweenv 1 4)))
            (assert! (andv (<v x y) (=v (+v x y) 5)))
            (all-values
              (solution (list x y)
                        (static-ordering #'linear-force))))))
    (format t "CONSTRAINT domain=finite-integer answers=~s~%" answers))

  (let ((left (make-variable "left"))
        (right (make-variable "right")))
    (format t "VARIABLE identity-distinct=~s left-bound=~s right-bound=~s~%"
            (not (eq left right))
            (bound? left)
            (bound? right)))

  (format t "BINARY ~a~%" (binary-bytes)))

(unless (uiop:getenv "SCREAMER_LAB_NO_RUN")
  (handler-case
      (progn
        (run-probe)
        (uiop:quit 0))
    (error (condition)
      (format *error-output* "ERROR ~a~%" condition)
      (uiop:quit 1))))
