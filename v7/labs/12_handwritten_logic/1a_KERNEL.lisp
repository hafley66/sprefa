(defpackage #:sprefa-lab-12
  (:use #:cl)
  (:export #:make-lvar #:walk #:occurs-p #:unify #:succeed #:fail #:disj #:conj
           #:fresh #:run #:fact #:horn #:relation #:take-stream #:reify-term))

(in-package #:sprefa-lab-12)

(defstruct (lvar (:constructor %make-lvar (id))) id)

(defvar *next-lvar-id* 0)

(defun make-lvar ()
  (%make-lvar (prog1 *next-lvar-id* (incf *next-lvar-id*))))

(defun variable-p (term)
  (typep term 'lvar))

(defun walk (term substitution)
  (loop
    (if (variable-p term)
        (let ((binding (assoc term substitution :test #'eq)))
          (if binding
              (setf term (cdr binding))
              (return term)))
        (return term))))

(defun occurs-p (variable term substitution)
  (setf term (walk term substitution))
  (cond
    ((eq variable term) t)
    ((consp term)
     (or (occurs-p variable (car term) substitution)
         (occurs-p variable (cdr term) substitution)))
    (t nil)))

(defun extend (variable term substitution)
  (and (not (occurs-p variable term substitution))
       (cons (cons variable term) substitution)))

(defun unify (left right substitution)
  (setf left (walk left substitution)
        right (walk right substitution))
  (cond
    ((eq left right) (values substitution t))
    ((variable-p left)
     (let ((next (extend left right substitution)))
       (values next (not (null next)))))
    ((variable-p right)
     (let ((next (extend right left substitution)))
       (values next (not (null next)))))
    ((and (consp left) (consp right))
     (multiple-value-bind (next matched-p)
         (unify (car left) (car right) substitution)
       (if matched-p
           (unify (cdr left) (cdr right) next)
           (values nil nil))))
    ((equal left right) (values substitution t))
    (t (values nil nil))))

(defun unit (state)
  (cons state nil))

(defun mplus (left right)
  (cond
    ((null left) right)
    ((functionp left)
     (lambda () (mplus right (funcall left))))
    (t
     (cons (car left)
           (lambda () (mplus right (cdr left)))))))

(defun bind (stream goal)
  (cond
    ((null stream) nil)
    ((functionp stream)
     (lambda () (bind (funcall stream) goal)))
    (t
     (mplus (funcall goal (car stream))
            (lambda () (bind (cdr stream) goal))))))

(defun succeed (state)
  (unit state))

(defun fail (state)
  (declare (ignore state))
  nil)

(defun disj (&rest goals)
  (lambda (state)
    (labels ((branches (remaining)
               (if remaining
                   (mplus (funcall (car remaining) state)
                          (lambda () (branches (cdr remaining))))
                   nil)))
      (branches goals))))

(defun conj (&rest goals)
  (lambda (state)
    (labels ((chain (remaining stream)
               (if remaining
                   (chain (cdr remaining) (bind stream (car remaining)))
                   stream)))
      (chain goals (unit state)))))

(defun fresh (count builder)
  (lambda (state)
    (let ((variables (loop repeat count collect (make-lvar))))
      (funcall (apply builder variables) state))))

(defun stream-pull (stream)
  (loop while (functionp stream) do (setf stream (funcall stream)) finally (return stream)))

(defun take-stream (limit stream)
  (let ((answers nil))
    (loop repeat limit do
      (setf stream (stream-pull stream))
      (unless stream (return))
      (push (car stream) answers)
      (setf stream (cdr stream)))
    (nreverse answers)))

(defun reify-term (term substitution &optional (bound 32))
  (let ((names nil))
    (labels ((name-for (variable)
               (or (cdr (assoc variable names :test #'eq))
                   (let ((name (list :var (length names))))
                     (push (cons variable name) names)
                     name)))
             (reify (value depth)
               (setf value (walk value substitution))
               (cond
                 ((zerop depth) :truncated)
                 ((variable-p value) (name-for value))
                 ((consp value)
                  (cons (reify (car value) (1- depth))
                        (reify (cdr value) (1- depth))))
                 (t value))))
      (reify term bound))))

(defun run (limit variables goal &optional (reification-bound 32))
  (mapcar (lambda (state)
            (mapcar (lambda (variable)
                      (reify-term variable state reification-bound))
                    variables))
          (take-stream limit (funcall goal nil))))

(defstruct clause head body)

(defun fact (&rest head)
  (make-clause :head head :body nil))

(defun horn (head body-builder)
  (make-clause :head head :body body-builder))

(defun relation (&rest clauses)
  (lambda (&rest query)
    (lambda (state)
      (labels ((attempt (clause)
                 (let ((renames nil))
                   (labels ((rename (variable)
                              (or (cdr (assoc variable renames :test #'eq))
                                  (let ((fresh-variable (make-lvar)))
                                    (push (cons variable fresh-variable) renames)
                                    fresh-variable)))
                            (copy-term (term)
                              (cond
                                ((variable-p term) (rename term))
                                ((consp term)
                                 (cons (copy-term (car term))
                                       (copy-term (cdr term))))
                                (t term))))
                     (multiple-value-bind (next matched-p)
                         (unify (copy-term (clause-head clause)) query state)
                       (when matched-p
                         (if (clause-body clause)
                             (funcall (funcall (clause-body clause) #'rename) next)
                             (unit next)))))))
               (try-clauses (remaining)
                 (if remaining
                     (mplus (attempt (car remaining))
                            (lambda () (try-clauses (cdr remaining))))
                     nil)))
        (try-clauses clauses)))))
