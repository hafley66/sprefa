(require :asdf)
(load (merge-pathnames "1a_MINIMAL.lisp" *load-truename*))
(load (merge-pathnames "1b_SUBPROCESS.lisp" *load-truename*))

(sprefa-lab-14-minimal:run-probe)
(sprefa-lab-14-subprocess:run-probe)
