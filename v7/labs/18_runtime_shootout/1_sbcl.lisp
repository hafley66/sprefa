;;;; Runtime shootout arm: SBCL native Common Lisp.
;;;;
;;;; Usage: sbcl --script 1_sbcl.lisp CASE N   (CASE in {chain, ring}, N > 0)
;;;;
;;;; Algorithm: build N deterministic directed edges as hash tables
;;;; (successor lists keyed by node id, values being materialized cons
;;;; pairs (from . to)). Reachability from each origin does a recursive
;;;; DFS with a per-origin hash-set of visited nodes so rings terminate;
;;;; every distinct (origin . to) closure pair reachable from the origin is
;;;; accumulated into the origin's result hash-set of cons pairs. The
;;;; closure count is the total number of materialized pairs across all
;;;; origins. Timing uses SBCL's monotonic real time (get-internal-real-
;;;; time), split into graph setup vs closure evaluation/materialization,
;;;; reported in milliseconds.

(defpackage #:runtime-shootout-sbcl
  (:use #:cl))
(in-package #:runtime-shootout-sbcl)

(defun now-ms ()
  "Monotonic real time in milliseconds."
  (* 1000.0d0
     (/ (get-internal-real-time)
        (float internal-time-units-per-second 1.0d0))))

(defun build-graph (case n)
  "Materialize the directed edges as a vector of successor cons-pair
lists: edges\[i\] holds every (i . to) pair. Returns (values edges edge-count)."
  (let ((edges (make-array n :initial-element nil))
        (edge-count 0))
    (dotimes (i n)
      (let ((to (if (string= case "ring")
                    (mod (1+ i) n)
                    (when (< i (1- n)) (1+ i)))))
        (when to
          (push (cons i to) (svref edges i))
          (incf edge-count))))
    (values edges edge-count)))

(defun closure-from (origin edges n)
  "Recursive DFS from ORIGIN over EDGES with a per-origin visited set.
Accumulates every distinct (ORIGIN . to) pair reachable from ORIGIN into a
hash set keyed by the cons pair itself (materialized host objects)."
  (let ((visited (make-hash-table :test #'eql))
        (pairs (make-hash-table :test #'equal)))
    (labels ((walk (node)
               (unless (>= node n)
                 (dolist (edge (svref edges node))
                   (let ((to (cdr edge)))
                     (setf (gethash (cons origin to) pairs) t)
                     (unless (gethash to visited)
                       (setf (gethash to visited) t)
                       (walk to)))))))
      (when (< origin n)
        (setf (gethash origin visited) t)
        (walk origin)))
    (hash-table-count pairs)))

(defun expected-count (case n)
  (if (string= case "ring")
      (* n n)
      (/ (* n (1- n)) 2)))

(defun die (msg)
  (format *error-output* "~A~%" msg)
  (sb-ext:exit :code 1))

(defun main ()
  (let ((args (rest sb-ext:*posix-argv*)))
    (unless (= (length args) 2)
      (die "usage: sbcl --script 1_sbcl.lisp CASE N"))
    (destructuring-bind (case-str n-str) args
      (unless (or (string= case-str "chain") (string= case-str "ring"))
        (die "CASE must be chain or ring"))
      (let ((n (parse-integer n-str :junk-allowed nil)))
        (unless (and n (> n 0))
          (die "N must be an integer greater than zero"))
        (let ((t0 (now-ms)))
          (multiple-value-bind (edges edge-count) (build-graph case-str n)
            (let ((t1 (now-ms)))
              (let ((closure-count
                      (loop for origin below n
                            sum (closure-from origin edges n))))
                (let ((t2 (now-ms))
                      (expected (expected-count case-str n)))
                  (unless (= closure-count expected)
                    (die (format nil "closure count mismatch: got ~D, expected ~D"
                                 closure-count expected)))
                  (format t
                          "{\"runtime\":\"sbcl\",\"version\":\"~A\",\"case\":\"~A\",\"n\":~D,\"edge_count\":~D,\"closure_count\":~D,\"setup_ms\":~,3F,\"closure_ms\":~,3F}~%"
                          (lisp-implementation-version)
                          case-str n edge-count closure-count
                          (- t1 t0) (- t2 t1)))))))))))

(main)
