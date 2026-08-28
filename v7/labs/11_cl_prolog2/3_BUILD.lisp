(require :asdf)

(defun required-environment-pathname (name)
  (let ((value (uiop:getenv name)))
    (unless value
      (error "Missing required environment variable ~A" name))
    (pathname value)))

(defparameter +upstream-pin+ "21531c553208e01c0b0b205ea005afaefa7057e3")

(defun command-output (&rest arguments)
  (string-trim '(#\Space #\Tab #\Newline #\Return)
               (uiop:run-program arguments :output :string :error-output :string)))

(defun verify-upstream (upstream-root)
  (let ((root (namestring upstream-root)))
    (unless (string= +upstream-pin+
                     (command-output "git" "-C" root "rev-parse" "HEAD"))
      (error "CLP2_UPSTREAM does not match pin ~A" +upstream-pin+))
    (unless (string= ""
                     (command-output "git" "-C" root "status" "--porcelain"))
      (error "CLP2_UPSTREAM checkout is dirty"))))

(defun verify-external-image-path (image-path)
  (let ((path (namestring image-path)))
    (unless (and (>= (length path) 13)
                 (string= "/private/tmp/" path :end1 13 :end2 13))
      (error "CLP2_LAB_IMAGE must be under /private/tmp/"))))

(let ((quicklisp-setup (required-environment-pathname "CLP2_QUICKLISP_SETUP"))
      (upstream-root (required-environment-pathname "CLP2_UPSTREAM"))
      (image-path (required-environment-pathname "CLP2_LAB_IMAGE")))
  (verify-upstream upstream-root)
  (verify-external-image-path image-path)
  (load quicklisp-setup)
  (asdf:load-asd (merge-pathnames "cl-prolog2.asd" upstream-root))
  (asdf:load-asd (merge-pathnames "swi/cl-prolog2.swi.asd" upstream-root))
  (asdf:load-system "cl-prolog2.swi")
  (load (merge-pathnames "2_PROBE.lisp" *load-truename*))
  (setf (symbol-value (find-symbol "*BINARY-PATH*" "CL-PROLOG2-LAB-11")) image-path)
  (sb-ext:save-lisp-and-die image-path
                            :executable t
                            :toplevel (symbol-function (find-symbol "MAIN" "CL-PROLOG2-LAB-11"))
                            :save-runtime-options t))
