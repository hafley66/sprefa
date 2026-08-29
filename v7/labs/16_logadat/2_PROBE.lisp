(defpackage #:sprefa-logadat-upstream
  (:use #:cl))

(defun load-logadat-upstream ()
  (let ((root (sb-ext:posix-getenv "LOGADAT_UPSTREAM")))
    (unless (and root
                 (>= (length root) 13)
                 (string= "/private/tmp/" root :end1 13 :end2 13))
      (error "LOGADAT_UPSTREAM must name a checkout under /private/tmp/"))
    (let ((source (merge-pathnames "logadat.lisp"
                                   (pathname (concatenate 'string root "/")))))
      (unless (probe-file source)
        (error "Pinned logadat source is unreadable: ~A" source))
      (let ((*package* (find-package '#:sprefa-logadat-upstream)))
        (handler-bind ((warning #'muffle-warning))
          (load source))))))

(load-logadat-upstream)

(defpackage #:sprefa-lab-16
  (:use #:cl))

(in-package #:sprefa-lab-16)

(defparameter *library-name* "logadat")
(defparameter *library-commit* "23fc43cc918e0aaac2aace1410e7283ef675153a")

(defun relation-values (query-result predicate)
  (mapcar (lambda (row)
            (symbol-name (second row)))
          (cdr (assoc predicate query-result :test #'eq))))

(defun canonical-set (values)
  (sort (remove-duplicates (copy-list values) :test #'string=) #'string<))

(defun shared-path ()
  (sprefa-logadat-upstream::logadat
    :facts (edge (a b) (b c) (c a) (c d))
    :rule (path (x y)
                (in (x y) edge))
    :rule (path (x y)
                (in (x z) edge)
                (in (z y) path))
    :query (path ('a y))))

(defun updated-path ()
  (sprefa-logadat-upstream::logadat
    :facts (edge (a b) (b c) (c a) (c e))
    :rule (path (x y)
                (in (x y) edge))
    :rule (path (x y)
                (in (x z) edge)
                (in (z y) path))
    :query (path ('a y))))

(defun retracted-path ()
  (sprefa-logadat-upstream::logadat
    :facts (edge (a b) (b c) (c a))
    :rule (path (x y)
                (in (x y) edge))
    :rule (path (x y)
                (in (x z) edge)
                (in (z y) path))
    :query (path ('a y))))

(defun duplicate-facts ()
  (sprefa-logadat-upstream::logadat
    :facts (edge (a b) (a b))
    :query (edge ('a y))))

(defun duplicate-rules ()
  (sprefa-logadat-upstream::logadat
    :facts (edge (a b))
    :rule (path (x y)
                (in (x y) edge))
    :rule (path (x y)
                (in (x y) edge))
    :query (path ('a y))))

(defun negative-host-predicate ()
  (sprefa-logadat-upstream::logadat
    :facts (node (a) (b) (c))
    :rule (not-edge (x y)
                    (in (x) node)
                    (in (y) node)
                    (not (member (list x y) '((a b)) :test #'equal)))
    :query (not-edge ('a y))))

(defun upstream-symbol-present-p (name)
  (let ((package (find-package '#:sprefa-logadat-upstream)))
    (multiple-value-bind (symbol status) (find-symbol name package)
      (declare (ignore status))
      (and symbol (eq (symbol-package symbol) package)))))

(defun binary-receipt ()
  (let ((path (sb-ext:posix-getenv "LOGADAT_OUT")))
    (if (and path (probe-file path))
        (with-open-file (stream path :direction :input
                                      :element-type '(unsigned-byte 8))
          (values path (file-length stream)))
        (values nil nil))))

(defun run-probe ()
  (let ((*package* (find-package '#:sprefa-lab-16))
        (*print-pretty* nil))
    (format t "PROBE library=~A commit=~A~%" *library-name* *library-commit*)
    (format t "UNIFY present=~A~%" (upstream-symbol-present-p "UNIFY"))
    (format t "OCCURS present=~A~%" (upstream-symbol-present-p "OCCURS"))
    (let ((path-values
            (relation-values (sb-ext:with-timeout 2 (shared-path)) 'path)))
      (format t "PATH termination=naive-fixed-point timeout-seconds=2 answers=~S~%"
              (canonical-set path-values)))
    (let ((asserted (canonical-set (relation-values (shared-path) 'path)))
          (updated (canonical-set (relation-values (updated-path) 'path)))
          (retracted (canonical-set (relation-values (retracted-path) 'path))))
      (format t "FACTS adapter=declaration-rebuild assert=~S update=~S retract=~S~%"
              asserted updated retracted)
      (format t "UPDATE adapter=declaration-rebuild-after-retraction answers=~S~%"
              retracted))
    (let ((fact-values (relation-values (duplicate-facts) 'edge))
          (rule-values (relation-values (duplicate-rules) 'path)))
      (format t "DUPLICATES facts-count=~D facts=~S derived-count=~D derived=~S~%"
              (length fact-values) fact-values (length rule-values) rule-values))
    (format t "NEG host-predicate-domain=(A B C) answers=~S~%"
            (canonical-set (relation-values (negative-host-predicate) 'not-edge)))
    (format t "DYNAMIC-API assert=~A update=~A retract=~A~%"
            (upstream-symbol-present-p "ASSERT")
            (upstream-symbol-present-p "UPDATE")
            (upstream-symbol-present-p "RETRACT"))
    (multiple-value-bind (path bytes) (binary-receipt)
      (if bytes
          (format t "BINARY path=~A bytes=~D~%" path bytes)
          (format t "BINARY blocker=LOGADAT_OUT-missing-or-unreadable~%")))
    (finish-output)))

(defun main ()
  (handler-case
      (progn
        (run-probe)
        (format t "IMAGE load-function=~A compile-function=~A cli-eval=not-exposed~%"
                (not (null (fboundp 'load)))
                (not (null (fboundp 'compile))))
        (finish-output)
        (sb-ext:exit :code 0))
    (error (condition)
      (format *error-output* "ERROR ~A~%" condition)
      (sb-ext:exit :code 1))))

(run-probe)
