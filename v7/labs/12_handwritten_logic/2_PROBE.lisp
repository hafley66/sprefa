(load (merge-pathnames "1a_KERNEL.lisp" *load-truename*))

(in-package #:sprefa-lab-12)

(defun eqo (left right)
  (lambda (state)
    (multiple-value-bind (next matched-p) (unify left right state)
      (if matched-p (succeed next) (fail state)))))

(defun canonical-set (terms)
  (sort (remove-duplicates (mapcar #'prin1-to-string terms) :test #'string=)
        #'string<))

(defun make-shared-graph (include-d-p)
  (let* ((edge (apply #'relation
                      (append (list (fact 'a 'b)
                                    (fact 'b 'c)
                                    (fact 'c 'a))
                              (if include-d-p (list (fact 'c 'd)) nil))))
         (path nil)
         (x (make-lvar))
         (y (make-lvar))
         (z (make-lvar)))
    (setf path
          (relation
           (horn (list x y)
                 (lambda (rename)
                   (funcall edge (funcall rename x) (funcall rename y))))
           (horn (list x y)
                 (lambda (rename)
                   (conj (funcall edge (funcall rename x) (funcall rename z))
                         (funcall path (funcall rename z) (funcall rename y)))))))
    path))

(defun bounded-path-values (path answer-limit)
  (let ((answer (make-lvar)))
    (canonical-set
     (mapcar #'first
             (run answer-limit (list answer) (funcall path 'a answer) 16)))))

(defun probe-unification ()
  (let ((left (make-lvar))
        (right (make-lvar)))
    (multiple-value-bind (substitution matched-p)
        (unify (list 'pair left (list 'g right))
               (list 'pair 'a (list 'g 'b))
               nil)
      (if matched-p
          (reify-term (list 'pair left (list 'g right)) substitution 16)
          :failed))))

(defun probe-occurs-check ()
  (let ((variable (make-lvar)))
    (multiple-value-bind (substitution matched-p)
        (unify variable (list 'f variable) nil)
      (declare (ignore substitution))
      (if matched-p :accepted :rejected))))

(defun probe-fairness ()
  (let ((answer (make-lvar)))
    (labels ((diverge (state)
               (declare (ignore state))
               (lambda () (diverge nil))))
      (first (first (run 1 (list answer)
                             (disj #'diverge (eqo answer 'right))))))))

(defun probe-fair-conjunction ()
  (let ((branch (make-lvar))
        (answer (make-lvar)))
    (labels ((diverge (state)
               (declare (ignore state))
               (lambda () (diverge nil)))
             (finish (state)
               (if (eq (walk branch state) 'block)
                   (diverge state)
                   (funcall (eqo answer 'right) state))))
      (first (first (run 1 (list answer)
                             (conj (disj (eqo branch 'block)
                                         (eqo branch 'win))
                                   #'finish)))))))

(defun binary-receipt ()
  (let ((path (sb-ext:posix-getenv "HANDWRITTEN_OUT")))
    (if (and path (probe-file path))
        (with-open-file (stream path :direction :input
                                      :element-type '(unsigned-byte 8))
          (values path (file-length stream)))
        (values nil nil))))

(defun run-probe ()
  (let ((*package* (find-package '#:sprefa-lab-12))
        (*print-pretty* nil)
        (path (make-shared-graph t))
        (updated-path (make-shared-graph nil)))
    (format t "PROBE library=handwritten-cl-kernel version=local~%")
    (format t "UNIFY ~S~%" (probe-unification))
    (format t "OCCURS occurs-check=~A~%" (probe-occurs-check))
    (format t "FAIR disjunction-left=diverge answer=~S conjunction-left=diverge answer=~S~%"
            (probe-fairness) (probe-fair-conjunction))
    (format t "PATH adapter=answer-limit-12 answers=~S~%"
            (bounded-path-values path 12))
    (format t "UPDATE adapter=rebuild-after-retraction answers=~S~%"
            (bounded-path-values updated-path 12))
    (multiple-value-bind (path bytes) (binary-receipt)
      (if bytes
          (format t "BINARY path=~A bytes=~D~%" path bytes)
          (format t "BINARY blocker=HANDWRITTEN_OUT-missing-or-unreadable~%")))
    (finish-output)))

(defun main ()
  (handler-case
      (progn (run-probe) (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))

(run-probe)
