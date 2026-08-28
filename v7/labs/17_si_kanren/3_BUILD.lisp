;;;; Lab 17 build: standalone SBCL image embedding si-kanren + a probe core.
;;;; Run: QL_SETUP=<setup.lisp> SIK_COMMIT=<sha> SIK_OUT=<path> \
;;;;   sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 3_BUILD.lisp

(require :sb-posix)
(require :asdf)

(defpackage #:sik-vendor
  (:use #:cl))
(in-package #:sik-vendor)

(let ((*package* (find-package '#:sik-vendor)))
  (load (or (sb-posix:getenv "QL_SETUP")
            "/private/tmp/sprefa-v7-lab17/.quicklisp/setup.lisp"))
  (funcall (find-symbol "QUICKLOAD" "QUICKLISP-CLIENT") :si-kanren :silent t))

(in-package #:sik-vendor)
(defun find-lib-package ()
  (or (find-if (lambda (p)
                 (multiple-value-bind (s f) (find-symbol "UNIFY" p)
                   (and f s)))
               (list-all-packages)
               :from-end t)
      (error "si-kanren symbols not found")))

(defpackage #:si-kanren-build
  (:use #:cl)
  (:import-from #:sik-vendor #:find-lib-package))
(in-package #:si-kanren-build)

(defparameter *out* (or (sb-posix:getenv "SIK_OUT")
                        "/private/tmp/sprefa-v7-lab17/si-kanren-lab-image"))
(defparameter *commit* (or (sb-posix:getenv "SIK_COMMIT") "unpinned"))

(defparameter *lib* (find-lib-package))

(defun sym (name) (find-symbol name *lib*))

(defmacro sik (name &rest args)
  (let ((s (find-symbol (string name) *lib*)))
    (if (macro-function s)
        (funcall (macro-function s) (cons s args) nil)
        `(,s ,@args))))

(defun collect (goal-fn)
  (let ((q-var nil))
    (let ((stream (sik call/empty-state
                       (sik call/fresh (lambda (q)
                                         (setf q-var q)
                                         (funcall goal-fn q))))))
      (let ((states (funcall (sym "TAKE-ALL") stream)))
        (mapcar (lambda (st) (funcall (sym "WALK*") q-var (funcall (sym "S-OF") st)))
                states)))))

(defun edgeo (x y)
  (sik conde ((sik == x 'a) (sik == y 'b))
             ((sik == x 'b) (sik == y 'c))
             ((sik == x 'c) (sik == y 'a))
             ((sik == x 'c) (sik == y 'd))))

(defun main ()
  (format t "PROBE library=si-kanren version=~A image=built~%" *commit*)
  (let ((answers (collect (lambda (p)
                            (sik fresh (x y)
                              (edgeo x y)
                              (sik == p (list x y)))))))
    (format t "EDGE ~S~%" (sort answers #'string< :key #'princ-to-string)))
  (sb-ext:exit :code 0))

(sb-ext:save-lisp-and-die *out* :executable t :toplevel #'main)
