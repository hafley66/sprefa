;;; cl-gambol standalone SBCL image build. Run:
;;;   GAMBOL_SRC=/path/to/cl-gambol sbcl --noinform --disable-debugger --script 3_BUILD.lisp
;;; Produces ./cl-gambol-lab and a smoke-test output.

(require :asdf)
(require :sb-posix)

(defparameter *gambol-src* (sb-posix:getenv "GAMBOL_SRC"))

(asdf:load-asd (merge-pathnames "gambol.asd" (pathname (concatenate 'string *gambol-src* "/"))))
(asdf:load-system "gambol")

(defpackage #:cl-gambol-image
  (:use #:cl)
  (:export #:main))

(in-package #:cl-gambol-image)

(defparameter *built* nil)

(defun smoke ()
  (gambol:*- (edge a b))
  (gambol:*- (edge b c))
  (gambol:*- (path ?x ?y) (edge ?x ?y))
  (princ (gambol:pl-solve-all '((path a ?x))))
  (terpri))

(defun main ()
  (if *built*
      (progn (smoke) (sb-ext:exit :code 0))
      (progn (setf *built* t)
             (sb-ext:save-lisp-and-die "cl-gambol-lab" :executable t :toplevel #'main))))

(main)
