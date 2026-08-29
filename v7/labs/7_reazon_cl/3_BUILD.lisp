;;;; reazon-cl saved executable image build. SBCL 2.6.7.
;;;;
;;;; REAZON_SRC=<checkout> QL_SETUP=<quicklisp setup.lisp> \
;;;; REAZON_LAB_DIR=<this lab dir> REAZON_LAB_OUT=<external output path> \
;;;; sbcl --noinform --no-sysinit --no-userinit --disable-debugger --script 3_BUILD.lisp
;;;;
;;;; The image is written to REAZON_LAB_OUT (outside Git). Probe execution is
;;;; suppressed during the build; the saved toplevel runs the same probe with
;;;; the same provenance checks, then measures the executable path supplied by
;;;; REAZON_LAB_BINARY.

(require :asdf)

(defparameter *ql-setup* (or (uiop:getenv "QL_SETUP")
                             (error "QL_SETUP not set")))
(defparameter *reazon-src* (or (uiop:getenv "REAZON_SRC")
                               (error "REAZON_SRC not set")))
(defparameter *lab-dir* (or (uiop:getenv "REAZON_LAB_DIR")
                            (error "REAZON_LAB_DIR not set")))
(defparameter *out* (or (uiop:getenv "REAZON_LAB_OUT")
                        (error "REAZON_LAB_OUT not set")))

(setf (uiop:getenv "REAZON_LAB_SUPPRESS_PROBE") "1")

(load (merge-pathnames "2_PROBE.lisp" (uiop:ensure-directory-pathname *lab-dir*)))

(sb-ext:save-lisp-and-die *out*
                          :executable t
                          :toplevel (lambda ()
                                      (handler-case
                                          (progn
                                            (funcall (find-symbol "PROBE" :reazon-lab))
                                            (uiop:quit 0))
                                        (error (c)
                                          (format *error-output* "ERROR ~a~%" c)
                                          (uiop:quit 1))))
                          :save-runtime-options t)
