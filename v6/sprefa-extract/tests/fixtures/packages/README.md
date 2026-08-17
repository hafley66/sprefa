The `package_edge` fixture workspace: one arm per manifest kind, plus the cases
that must produce NOTHING.

  crates/alpha  -> beta (normal), gamma (dev, via the `package = "gamma"`
                  rename), gamma (build). The same pair twice under two kinds is
                  what proves `kind` is part of the edge key.
  crates/*      serde / serde_json are registry packages: no manifest here
                  declares them, so they are no edges.
  Cargo.toml    a virtual workspace root: no [package] name, so it is never an
                  edge target even though it is supplied.
  js/app        -> lib (normal), tools (dev), peer (peer). rxjs is external.
  go/svc        -> lib twice: `require` and a directory `replace` are two facts
                  about one pair. golang.org/x/net is external.
  go/tool       -> svc (require) and -> lib (replace naming a MODULE PATH rather
                  than a directory).
  broken/       unparseable TOML: contributes no name and no edges, which is the
                  degradation direction a best-effort manifest reader takes.
