(load (merge-pathnames "2_PROBE.lisp" *load-truename*))

(let ((output (sb-ext:posix-getenv "LOGADAT_OUT")))
  (unless (and output
               (>= (length output) 13)
               (string= "/private/tmp/" output :end1 13 :end2 13))
    (error "LOGADAT_OUT must name an artifact under /private/tmp/"))
  (sb-ext:save-lisp-and-die
   output
   :executable t
   :toplevel #'sprefa-lab-16::main
   :save-runtime-options t))
