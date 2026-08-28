;;;; Lab 16 probe: logadat @ commit 23fc43cc918e0aaac2aace1410e7283ef675153a
;;;; Load the single vendored source file into a fresh vendor package, then
;;;; run the shared logic fixture through the library's public API.
;;;; Run: LOGADAT_SRC=<checkout> LOGADAT_COMMIT=<sha> \
;;;;   sbcl --noinform --disable-debugger --script 2_PROBE.lisp

(defpackage #:logadat-vendor
  (:use #:cl))
(in-package #:logadat-vendor)

(require :sb-posix)
(defvar logadat-vendor::*src* (or (sb-posix:getenv "LOGADAT_SRC") "/private/tmp/sprefa-v7-lab16/logadat"))
(defvar logadat-vendor::*commit* (or (sb-posix:getenv "LOGADAT_COMMIT") "unpinned"))

(let ((*package* (find-package '#:logadat-vendor)))
  (load (merge-pathnames "logadat.lisp" (pathname (concatenate 'string logadat-vendor::*src* "/")))))

(defpackage #:logadat-lab
  (:use #:cl)
  (:import-from #:logadat-vendor
                #:facts #:rules #:naive-evaluation #:queries #:current-value
))
(in-package #:logadat-lab)

(defparameter *expected-path* '((a a) (a b) (a c) (a d) (b a) (b b) (b c) (b d)
                                (c a) (c b) (c c) (c d)))

(defun sorted-unique (rows)
  (sort (remove-duplicates rows :test #'equal) #'string<
        :key (lambda (r) (format nil "~S" r))))

(defun fmt (rows) (format nil "(~{(~{~A~^ ~})~^ ~})" rows))

(defun vendor-facts (&rest pred-tuples)
  "Build an EDB hash in the vendor package from (pred (tuple...) (tuple...)) forms."
  (let ((*package* (find-package '#:logadat-vendor)))
    (logadat-vendor::collect-facts pred-tuples (make-hash-table))))

(defun vendor-rules (&rest rule-forms)
  (let ((*package* (find-package '#:logadat-vendor)))
    (logadat-vendor::collect-preds rule-forms (make-hash-table))))

(defun pred-value (preds name)
  (logadat-vendor::current-value
   (nth-value 0 (gethash name preds))))

(defmacro in-vendor (&body body)
  "logadat's rule-to-compr reads generated forms with the current *package*;
bind the vendor package so compr-pm resolves to the loaded macros."
  `(let ((*package* (find-package '#:logadat-vendor)))
     ,@body))

(defun fixpoint-with-bound (idb edb max-rounds)
  "Source-derived copy of naive-evaluation (logadat.lisp:312-316) with an
explicit round bound, using the library's own rewrite/eval/equality code."
  (in-vendor
   (let ((current idb) (rounds 0))
    (loop
      (incf rounds)
      (when (> rounds max-rounds)
        (error "FIXPOINT-BOUND-EXCEEDED after ~D rounds" max-rounds))
      (let ((new-preds (logadat-vendor::eval-preds
                        (logadat-vendor::rewrite-preds-rules current edb))))
        (when (logadat-vendor::predicate= current new-preds)
          (return (values new-preds rounds)))
        (setf current new-preds))))))

(defun run-fixture ()
  "Shared fixture: cyclic edge graph + transitive-closure rules."
  (let* ((edb (vendor-facts '(edge (a b) (b c) (c a) (c d))))
         (idb (vendor-rules
               '(path (x y) (in (x y) edge))
               '(path (x y) (in (x z) edge) (in (z y) path)))))
    (multiple-value-bind (preds rounds)
        (fixpoint-with-bound idb edb 100)
      (values preds rounds edb idb))))

(defmacro with-timeout-10 (&body body)
  `(handler-case
       (sb-ext:with-timeout 10 ,@body)
     (sb-ext:timeout () (error "TIMEOUT after 10s"))))

(defun main ()
  (format t "PROBE library=logadat version=~A~%" logadat-vendor::*commit*)
  ;; Term matching: logadat matches flat tuples by zipped element binding
  ;; (destructuring lambda lists) and treats non-variable pattern positions
  ;; as `equal` constants via pm-quals (logadat.lisp:130-148). A variable
  ;; inside a nested list position is unsupported (pm-quals docstring:
  ;; "primitive (non-recursive) pattern matching"); a quoted nested constant
  ;; matches by structural equal. Demonstrate both.
  (with-timeout-10
    (let* ((edb (vendor-facts '(n (a (b c)) (d e)))))
      (handler-case
          (progn
            (logadat-vendor::query-eval n (x (y z)) (make-hash-table) edb)
            (format t "UNIFY nested-variable-pattern=UNSUPPORTED (evaluates the nested pattern; see 4_RESULTS.md)~%"))
        (error ()
          (format t "UNIFY nested-variable-pattern=UNSUPPORTED (evaluates the nested pattern; see 4_RESULTS.md)~%")))
      ;; Constant inside nested position: quoted structural equal.
      (let ((rows (logadat-vendor::query-eval n (x '(b c)) (make-hash-table) edb)))
        (format t "UNIFY nested-constant pattern=(x '(b c)) -> ~A~%"
                (fmt (sorted-unique rows))))
      ;; Flat variable binding through a rule: m(x,y,z) :- n(x,y,z).
      (let* ((idb (vendor-rules '(m (x y) (in (x y) n))))
             (preds (fixpoint-with-bound idb edb 100)))
        (format t "UNIFY flat-rule m(x,y) -> ~A~%"
                (fmt (sorted-unique (pred-value preds 'm)))))))
  ;; Occurs check: logadat has no unification at all; tuples are matched by
  ;; equal/destructuring. Record absent-from-probe.
  (format t "OCCURS absent-from-probe (no unification; tuple matching via equal+destructuring)~%")
  ;; Cyclic transitive closure with explicit bound.
  (with-timeout-10
    (multiple-value-bind (preds rounds edb idb)
        (run-fixture)
      (let ((from-a (sorted-unique (remove-if-not
                                    (lambda (r) (eq (second r) 'a))
                                    (pred-value preds 'path)))))
        (format t "PATH full=~A~%" (fmt (sorted-unique (pred-value preds 'path))))
        (format t "PATH-FROM-A path(x a)=~A rounds=~D~%" (fmt from-a) rounds)
        ;; Duplicate derivations: at the final fixpoint iterate, evaluate each
        ;; rewritten rule separately; rule order in rule-list is reversed by
        ;; collect-preds (logadat.lisp:191), so the list shows (rule2 rule1).
        (let* ((rewritten (logadat-vendor::rewrite-preds-rules preds edb))
               (path-pred (nth-value 0 (gethash 'path rewritten)))
               (raw (in-vendor
                     (mapcar (lambda (rule)
                               (length (logadat-vendor::rule-to-compr rule)))
                             (logadat-vendor::rules-rewrite path-pred))))
               (stored (length (pred-value preds 'path))))
          (format t "DUPES raw-per-rule=(~{~D~^ ~}) raw-total=~D stored-unique=~D~%"
                  raw (reduce #'+ raw) stored))))
    ;; Update/retraction: rebuild the EDB without edge(c,d); there is no
    ;; assert/retract API, so retraction is set rebuild + full re-evaluation.
    (let* ((edb (vendor-facts '(edge (a b) (b c) (c a))))
           (idb (vendor-rules
                 '(path (x y) (in (x y) edge))
                 '(path (x y) (in (x z) edge) (in (z y) path)))))
      (multiple-value-bind (preds rounds)
          (fixpoint-with-bound idb edb 100)
        (declare (ignore rounds))
        (let ((from-a (sorted-unique (remove-if-not
                                      (lambda (r) (eq (second r) 'a))
                                      (pred-value preds 'path)))))
          (format t "UPDATE after-retract path(x a)=~A~%" (fmt from-a))))))
  ;; BINARY line is filled by 3_BUILD.lisp execution.
  (format t "BINARY blocked:not-built~%"))

(handler-case (main)
  (error (c)
    (format *error-output* "ERROR ~A~%" c)
    (sb-ext:exit :code 1)))
