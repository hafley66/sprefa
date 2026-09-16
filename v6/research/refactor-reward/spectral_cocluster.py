#!/usr/bin/env python3
"""Spectral co-clustering (Dhillon 2001) on the Engine method×field matrix.

The god-object study found a 44/48/18 partition (db_only/stateful/pure_fn)
via hand-rolled field-coverage heuristics: peel the `db` column, look for
methods that touch ONLY db. This script asks: does linear algebra find the
same partition for free, without the manual hub-peeling step?

Dhillon's algorithm:
  1. Build bipartite adjacency matrix A (methods × fields), binary.
  2. Normalize: A_n = D_m^{-1/2} A D_f^{-1/2}  (D_m, D_f = degree matrices).
  3. SVD: A_n = U Σ V^T. Top singular vectors encode the co-clustering.
  4. Project methods onto the top 2 right singular vectors of A_n^T A_n.
  5. k-means on the embedding → cluster assignment.

No sklearn needed. Pure numpy SVD + a 2-line k-means on 2D points.
"""
import subprocess, sys, re, os
import numpy as np

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

def dl_query(dl_file, query_name):
    """Run a dl query and return rows as lists of strings."""
    out = subprocess.check_output(
        ["target/debug/dl", dl_file, "--root", "v5", "--db", "/tmp/dl-svd.db",
         "--no-daemon"],
        text=True, env={**os.environ, "SPREFA_SCIP_INDEX": "./index.scip"},
        cwd=REPO,
    )
    # Parse: lines after "? <query_name> => ..." until blank or next "?"
    lines = out.splitlines()
    rows = []
    capturing = False
    for line in lines:
        if line.startswith(f"? {query_name}"):
            capturing = True
            continue
        if capturing:
            if line.startswith("? ") or not line.strip() or line.strip().startswith("("):
                break
            # row format: "  val1.\tval2." or "  val1."
            stripped = line.strip()
            parts = [p.rstrip(".") for p in stripped.split("\t")]
            rows.append(parts)
    return rows

def build_matrix():
    methods_raw = dl_query("examples/field_matrix.dl", "method_name")
    fields_raw = dl_query("examples/field_matrix.dl", "field_name")
    refs_raw = dl_query("examples/field_matrix.dl", "eng_field_ref")

    methods = sorted(set(r[0] for r in methods_raw))
    fields = sorted(set(r[0] for r in fields_raw))
    m_idx = {m: i for i, m in enumerate(methods)}
    f_idx = {f: i for i, f in enumerate(fields)}

    A = np.zeros((len(methods), len(fields)))
    for row in refs_raw:
        fn, field = row[0], row[1]
        if fn in m_idx and field in f_idx:
            A[m_idx[fn], f_idx[field]] = 1.0

    return A, methods, fields

def short_name(sym):
    """Extract method short name from the full moniker."""
    m = re.search(r'(\w+)\(\)\.$', sym)
    return m.group(1) if m else sym

def kmeans_2d(points, k, max_iter=100, seed=42):
    rng = np.random.default_rng(seed)
    centroids = points[rng.choice(len(points), k, replace=False)]
    for _ in range(max_iter):
        dists = np.linalg.norm(points[:, None] - centroids[None, :], axis=2)
        labels = np.argmin(dists, axis=1)
        new_centroids = np.array([points[labels == j].mean(axis=0)
                                   if np.any(labels == j) else centroids[j]
                                   for j in range(k)])
        if np.allclose(new_centroids, centroids):
            break
        centroids = new_centroids
    return labels

def main():
    A, methods, fields = build_matrix()
    n_methods, n_fields = A.shape
    print(f"Matrix: {n_methods} methods × {n_fields} fields, {int(A.sum())} edges\n")

    # Field degree (column sums)
    field_deg = A.sum(axis=0)
    print("Field degrees:")
    for j, f in enumerate(fields):
        bar = "█" * int(field_deg[j])
        print(f"  {f:30s} {int(field_deg[j]):3d} {bar}")
    print()

    # Method degree (row sums)
    method_deg = A.sum(axis=1)
    print("Method degree distribution:")
    for d in sorted(set(method_deg.astype(int))):
        count = int(np.sum(method_deg == d))
        print(f"  degree {d}: {count} methods")
    print()

    # ── Dhillon spectral co-clustering ────────────────────────────────
    # Normalized bipartite: A_n = D_m^{-1/2} A D_f^{-1/2}
    D_m = np.diag(1.0 / np.sqrt(np.maximum(method_deg, 1e-10)))
    D_f = np.diag(1.0 / np.sqrt(np.maximum(field_deg, 1e-10)))
    A_n = D_m @ A @ D_f

    # SVD
    U, S, Vt = np.linalg.svd(A_n, full_matrices=False)
    print(f"Singular values: {S[:5]}")

    # Method embedding: top 2 left singular vectors (excluding the first,
    # which is the trivial component). Dhillon uses the 2nd and 3rd.
    if len(S) >= 3:
        emb = U[:, 1:3]
    elif len(S) >= 2:
        emb = U[:, 1:2]
    else:
        print("Not enough singular values for embedding.")
        return

    # k-means with k=2 (the db_only vs stateful split)
    labels_2 = kmeans_2d(emb, k=2)

    print("\n" + "="*70)
    print("k=2 co-clustering (spectral):")
    print("="*70)
    for cluster in range(2):
        members = [i for i in range(n_methods) if labels_2[i] == cluster]
        degrees = [int(method_deg[i]) for i in members]
        fields_used = set()
        for i in members:
            for j in range(n_fields):
                if A[i, j]:
                    fields_used.add(fields[j])
        print(f"\nCluster {cluster}: {len(members)} methods")
        print(f"  Fields touched: {sorted(fields_used)}")
        print(f"  Avg degree: {np.mean(degrees):.1f}, Min: {min(degrees)}, Max: {max(degrees)}")
        print(f"  Members (sorted by degree):")
        for i in sorted(members, key=lambda x: method_deg[x]):
            name = short_name(methods[i])
            deg = int(method_deg[i])
            fs = [fields[j] for j in range(n_fields) if A[i, j]]
            print(f"    {deg}  {name:40s}  {fs}")

    # ── Compare with known db_only partition ──────────────────────────
    # db_only = methods touching ONLY the db field
    f_idx = {f: i for i, f in enumerate(fields)}
    db_col = f_idx.get("db", -1)
    if db_col >= 0:
        db_only_mask = (A.sum(axis=1) == 1) & (A[:, db_col] == 1)
        pure_fn_mask = (A.sum(axis=1) == 0)
        stateful_mask = ~(db_only_mask | pure_fn_mask)

        print("\n" + "="*70)
        print("Known partition (field-coverage heuristic):")
        print("="*70)
        for name, mask in [("db_only", db_only_mask),
                           ("stateful", stateful_mask),
                           ("pure_fn (no fields)", pure_fn_mask)]:
            members = [i for i in range(n_methods) if mask[i]]
            print(f"  {name}: {len(members)} methods")

        # Cross-tab: spectral cluster × known partition
        print("\n" + "="*70)
        print("Cross-tab: spectral k=2 × known partition:")
        print("="*70)
        known_labels = np.where(db_only_mask, 0,
                      np.where(pure_fn_mask, 2, 1))
        print(f"  {'':20s}  spectral_0  spectral_1  total")
        for name, k in [("db_only", 0), ("stateful", 1), ("pure_fn", 2)]:
            counts = [int(np.sum((known_labels == k) & (labels_2 == 0))),
                       int(np.sum((known_labels == k) & (labels_2 == 1)))]
            print(f"  {name:20s}  {counts[0]:10d}  {counts[1]:11d}  {sum(counts)}")

    # ── k=3 (does it find all 3?) ─────────────────────────────────────
    labels_3 = kmeans_2d(emb, k=3)
    print("\n" + "="*70)
    print("k=3 co-clustering (spectral):")
    print("="*70)
    for cluster in range(3):
        members = [i for i in range(n_methods) if labels_3[i] == cluster]
        degrees = [int(method_deg[i]) for i in members]
        avg_d = np.mean(degrees) if degrees else 0
        print(f"  Cluster {cluster}: {len(members)} methods, avg degree {avg_d:.1f}")

    # ── Fiedler vector of the method co-occurrence graph ───────────────
    # Method-method graph: two methods are connected if they share a field
    M = A @ A.T  # co-occurrence count
    np.fill_diagonal(M, 0)
    adj = (M > 0).astype(float)
    degree = adj.sum(axis=1)
    D = np.diag(np.maximum(degree, 1e-10))
    L = D - adj
    eigvals, eigvecs = np.linalg.eigh(L)
    print(f"\nLaplacian eigenvalues: {eigvals[:5]}")
    if len(eigvals) >= 2:
        fiedler = eigvecs[:, 1]
        sorted_idx = np.argsort(fiedler)
        print("\nFiedler vector ranking (methods sorted by Fiedler coordinate):")
        print(f"  {'rank':4s}  {'fiedler':>10s}  {'degree':>6s}  {'method':40s}  fields")
        for rank, i in enumerate(sorted_idx):
            name = short_name(methods[i])
            deg = int(method_deg[i])
            fs = [fields[j] for j in range(n_fields) if A[i, j]]
            print(f"  {rank:4d}  {fiedler[i]:10.4f}  {deg:6d}  {name:40s}  {fs}")

if __name__ == "__main__":
    main()
