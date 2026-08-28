;;;; Lab 17 probe: si-kanren @ 93f051fcc2b46649d214eab951cdd4ed1de869da
;;;; Quicklisp dist 20260101 (tarball tree-identical to the upstream commit).
;;;; Run: QL_SETUP=<setup.lisp> SIK_COMMIT=<sha> \
;;;;   sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 2_PROBE.lisp

(require :sb-posix)
(require :asdf)

(defpackage #:sik-vendor
  (:use #:cl))
(in-package #:sik-vendor)

(let ((*package* (find-package '#:sik-vendor)))
  (load (or (sb-posix:getenv "QL_SETUP")
            "/private/tmp/sprefa-v7-lab17/.quicklisp/setup.lisp"))
  (funcall (find-symbol "QUICKLOAD" "QUICKLISP-CLIENT") :si-kanren :silent t))

(defun find-lib-package ()
  (or (find-if (lambda (p)
                 (multiple-value-bind (s f) (find-symbol "UNIFY" p)
                   (and f s)))
               (list-all-packages)
               :from-end t)
      (error "si-kanren symbols not found")))

(defpackage #:si-kanren-lab
  (:use #:cl)
  (:import-from #:sik-vendor #:find-lib-package))
(in-package #:si-kanren-lab)

(defparameter *lib* (find-lib-package))

(defun sym (name) (find-symbol name *lib*))

;; The library has no defpackage/exports; this wrapper expands a lab-side call
;; into the library macro of the same name, or a direct call for functions,
;; keeping every library symbol package-qualified.
(defmacro sik (name &rest args)
  (let ((s (find-symbol (string name) *lib*)))
    (if (macro-function s)
        (funcall (macro-function s) (cons s args) nil)
        `(,s ,@args))))

(defun sorted-unique (rows)
  (sort (remove-duplicates rows :test #'equal) #'string<
        :key (lambda (r) (format nil "~S" r))))

(defun fmt (rows)
  (format nil "(~{~A~^ ~})"
          (mapcar (lambda (r) (if (atom r) (format nil "(~A)" r) (format nil "(~{~A~^ ~})" r)))
                  rows)))

;; Observed upstream bug (commit 93f051f, si-kanren.lisp:57): the equal-value
;; branch of unify returns s, but with the empty substitution s = NIL the cond
;; test value is NIL, cond falls through to failure. Any == between identical
;; ground atoms or equal compound heads therefore fails while the substitution
;; is still empty. Adapter: bind one dummy variable first so s is non-empty.
(defun g== (u v)
  (sik conj+
       (sik fresh (d) (sik == d 'd0))
       (sik == u v)))

(defmacro with-timeout (secs &body body)
  `(handler-case
       (sb-ext:with-timeout ,secs ,@body)
     (sb-ext:timeout () 'timeout)))

;; The library's run/run* only PRINT reified answers (mK-reify,
;; wrappers.lisp:107-115, returns no values), so the probe collects answer
;; states itself and walks the query variable out of each state.
(defun collect-n (n goal-fn)
  (let ((q-var nil))
    (let ((stream (sik call/empty-state
                       (sik call/fresh (lambda (q)
                                         (setf q-var q)
                                         (funcall goal-fn q))))))
      (let ((states (if n (funcall (sym "TAKE") n stream)
                        (funcall (sym "TAKE-ALL") stream))))
        (mapcar (lambda (st) (funcall (sym "WALK*") q-var (funcall (sym "S-OF") st)))
                states)))))

(defun collect (goal-fn) (collect-n nil goal-fn))

;; ---- fixture relations --------------------------------------------------
;; edge(a b) edge(b c) edge(c a) edge(c d)
(defun edgeo (x y)
  (sik conde ((g== x 'a) (g== y 'b))
             ((g== x 'b) (g== y 'c))
             ((g== x 'c) (g== y 'a))
             ((g== x 'c) (g== y 'd))))

;; patho_k: k-step transitive path via explicit depth enumeration (adapter;
;; the library has no tabling, so an unbounded patho diverges).
(defun patho1 (x y) (edgeo x y))
(defun patho2 (x y)
  (sik disj+ (edgeo x y)
             (sik fresh (z) (edgeo x z) (sik zzz (patho1 z y)))))
(defun patho3 (x y)
  (sik disj+ (edgeo x y)
             (sik fresh (z) (edgeo x z) (sik zzz (patho2 z y)))))
(defun patho4 (x y)
  (sik disj+ (edgeo x y)
             (sik fresh (z) (edgeo x z) (sik zzz (patho3 z y)))))

;; Unbounded path: diverges; used for the timeout probe.
(defun patho* (x y)
  (sik disj+ (edgeo x y)
             (sik fresh (z) (edgeo x z) (sik zzz (patho* z y)))))

;; appendo in both directions.
(defun appendo (l1 l2 out)
  (sik disj+
       (sik conj+ (g== l1 '()) (g== l2 out))
       (sik fresh (u v w)
         (sik == l1 (cons u v))
         (sik == out (cons u w))
         (appendo v l2 w))))

;; Infinite recursion, no answers: for the fairness probe.
(defun loopy () (sik fresh () (sik zzz (loopy))))

(defun main ()
  (format t "PROBE library=si-kanren version=~A pkg=~A~%"
          (or (sb-posix:getenv "SIK_COMMIT") "unpinned") (package-name *lib*))
  ;; Observed-bug receipt: identical ground atoms unify to failure on the
  ;; empty substitution (si-kanren.lisp:57 cond fall-through).
  (format t "BUG ground==ground empty-s result=~S~%"
          (collect (lambda (q) (sik == 'a 'a))))
  ;; Nested unification: f(q g(r)) = f(a g(b)) binds q=a and r=b, via the
  ;; g== adapter.
  (let ((target '(f a (g b))))
    (format t "UNIFY qr=~A~%"
            (collect (lambda (p)
                       (sik fresh (q r)
                         (g== (list 'f q (list 'g r)) target)
                         (g== p (list q r))))))
  ;; Occurs check: X = (X) must fail. unify (si-kanren.lisp:46-58) calls
  ;; occurs? unconditionally; failure is the '(()) marker.
  (format t "OCCURS occurs-check=T policy=unconditional-failure result=~S~%"
          (collect (lambda (q) (sik == q (list q)))))
  ;; Answer order follows conde clause order; duplicates preserved.
  (format t "ORDER-DUPES ~A~%"
          (fmt (collect (lambda (q)
                          (sik disj+ (g== q 5) (g== q 5))))))
  ;; Bidirectional append.
  (format t "APPEND-LHS ~A~%" (fmt (collect (lambda (q) (appendo '(a b) '(c d) q)))))
  (format t "APPEND-RHS ~A~%" (fmt (collect (lambda (q) (appendo q '(c d) '(a b c d))))))
  ;; Fairness: infinite left branch must not starve a finite right branch.
  (format t "FAIR cap=1 answers=~A bound=5s~%"
          (with-timeout 5
            (collect-n 1 (lambda (q)
                           (sik disj+ (loopy) (g== q 'done))))))
  ;; Cyclic transitive closure via depth-bounded adapter (k = 1..4).
  (flet ((k-rows (k goal)
           (remove-if-not (lambda (r) (eq (car r) 'a))
                          (collect (lambda (p)
                                     (sik fresh (x y)
                                       (funcall goal x y)
                                       (g== p (list x y))))))))
    (let ((rows (append (k-rows 1 #'patho1) (k-rows 2 #'patho2)
                        (k-rows 3 #'patho3) (k-rows 4 #'patho4))))
      (format t "PATH-FROM-A sorted-unique=~A k-max=4~%" (fmt (sorted-unique rows)))))
  ;; Unbounded recursion diverges: explicit timeout bound.
  (format t "PATH-UNBOUNDED result=~S bound=5s n=100000~%"
          (with-timeout 5
            (length (collect-n 100000
                               (lambda (p)
                                 (sik fresh (x y)
                                   (patho* x y)
                                   (g== p (list x y)))))))))
  ;; Constraints on the fixture domain.
  (format t "DISEQ ~A~%"
          (fmt (collect (lambda (q)
                          (sik conj+
                               (sik disj+ (g== q 'a) (g== q 'b))
                               (sik =/= q 'a))))))
  (format t "NUMBERO ~A~%"
          (fmt (collect (lambda (q) (sik conj+ (sik numbero q) (g== q 5))))))
  (format t "SYMOLO ~A~%"
          (fmt (collect (lambda (q) (sik conj+ (sik symbolo q) (g== q 'a))))))
  (format t "ABSENTO ~A~%"
          (fmt (collect (lambda (q) (sik conj+ (sik absento 'c q) (g== q '(a b)))))))
  (format t "ABSENTO-FORBIDDEN ~A~%"
          (collect (lambda (q) (sik conj+ (sik absento 'c q) (g== q '(a c))))))
  (format t "BINARY blocked:not-built~%"))

(handler-case (main)
  (error (c)
    (format *error-output* "ERROR ~A~%" c)
    (sb-ext:exit :code 1)))
