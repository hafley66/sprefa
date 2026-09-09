(require :asdf)

(defun required-external-path (name)
  (let ((value (uiop:getenv name)))
    (unless (and value (uiop:directory-exists-p (pathname value)))
      (error "~A must name an existing directory under /private/tmp/" name))
    (unless (uiop:string-prefix-p "/private/tmp/" value)
      (error "~A must be under /private/tmp/" name))
    (pathname value)))

(defun external-output-path (directory filename)
  (merge-pathnames filename (uiop:ensure-directory-pathname directory)))

(let* ((directory (required-external-path "LAB14_OUT"))
       (shape (or (uiop:getenv "LAB14_SHAPE")
                  (error "LAB14_SHAPE is required"))))
  (cond
    ((string= shape "minimal")
     (load (merge-pathnames "1a_MINIMAL.lisp" *load-truename*))
     (sb-ext:save-lisp-and-die
      (external-output-path directory "minimal-sbcl")
      :executable t
      :toplevel (symbol-function (find-symbol "MAIN" "SPREFA-LAB-14-MINIMAL"))
      :save-runtime-options t))
    ((string= shape "subprocess")
     (load (merge-pathnames "1b_SUBPROCESS.lisp" *load-truename*))
     (sb-ext:save-lisp-and-die
      (external-output-path directory "sbcl-subprocess-swi")
      :executable t
      :toplevel (symbol-function (find-symbol "MAIN" "SPREFA-LAB-14-SUBPROCESS"))
      :save-runtime-options t))
    (t
     (error "Unknown LAB14_SHAPE ~S" shape))))
