;;;; Save one SBCL executable image to BINARY_PACKAGING_OUT.
;;;;
;;;; Required environment:
;;;;   BINARY_PACKAGING_SHAPE=minimal-sbcl|sbcl-swi-subprocess
;;;;   BINARY_PACKAGING_OUT=/private/tmp/...

(require :asdf)

(defparameter *lab-directory*
  (uiop:pathname-directory-pathname (or *load-truename* (uiop:argv0))))

(defparameter *shape*
  (or (uiop:getenv "BINARY_PACKAGING_SHAPE")
      (error "set BINARY_PACKAGING_SHAPE")))

(defparameter *output*
  (or (uiop:getenv "BINARY_PACKAGING_OUT")
      (error "set BINARY_PACKAGING_OUT")))

(unless (and (>= (length *output*) 13)
             (string= "/private/tmp/" *output* :end1 13 :end2 13))
  (error "BINARY_PACKAGING_OUT must be under /private/tmp/"))

(ensure-directories-exist (pathname *output*))
(load (merge-pathnames "4_CL_IMAGE_MAIN.lisp" *lab-directory*))

(let ((toplevel
        (cond
          ((string= *shape* "minimal-sbcl")
           #'binary-packaging-lab:main-minimal)
          ((string= *shape* "sbcl-swi-subprocess")
           #'binary-packaging-lab:main-swi-subprocess)
          (t
           (error "unknown BINARY_PACKAGING_SHAPE: ~A" *shape*)))))
  (sb-ext:save-lisp-and-die
   *output*
   :executable t
   :toplevel toplevel
   :save-runtime-options t))
