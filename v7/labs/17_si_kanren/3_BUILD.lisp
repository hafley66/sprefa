(let ((quicklisp-setup (sb-ext:posix-getenv "SI_KANREN_QUICKLISP"))
      (output (sb-ext:posix-getenv "SI_KANREN_OUT")))
  (unless (and quicklisp-setup (probe-file quicklisp-setup))
    (error "SI_KANREN_QUICKLISP must name a readable setup.lisp"))
  (unless (and output
               (>= (length output) 13)
               (string= "/private/tmp/" output :end1 13 :end2 13))
    (error "SI_KANREN_OUT must name an artifact under /private/tmp/"))
  (load quicklisp-setup)
  (funcall (intern "QUICKLOAD" "QL") "si-kanren")
  (load (merge-pathnames "2_PROBE.lisp" *load-truename*))
  (let ((main (find-symbol "MAIN" "DL7-SI-KANREN-LAB")))
    (sb-ext:save-lisp-and-die
     output
     :executable t
     :toplevel main
     :save-runtime-options t)))
