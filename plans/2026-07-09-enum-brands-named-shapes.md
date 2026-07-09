# Enum brands + named shapes (type-system prototype)

Two typecheck-time rungs riding the existing `type X <: Y` brand machinery. No
`lower.rs` / engine-tick / SQL change: columns stay text at runtime, brands and
shapes are load-time-only.

## Surface

    type severity = "error" | "warn" | "info" | "hint".   -- RUNG 1: enum brand
    type finding(path: text, line: int, sev: severity).   -- RUNG 2: named shape
    rel finding_rel: finding.                              -- shape-referencing rel

Enum literals are STRING literals (`"error"`), not `:error` atoms — the `:ident`
form lexes as `Colon` + `Ident`, not a standalone token, so string literals are
the fallback the spec sanctions. The only new lexer surface is `|` (`Tok::Pipe`),
intrinsic to the enum separator, not the atom form.

`type` now has three arms, disambiguated on the token AFTER the name:
  `<:` (Lt2) -> brand parent form   `=` (Eq) -> enum brand   `(` (LParen) -> shape.

## Type signatures (first)

    // ast.rs
    struct BrandDecl { name: String, parent: String, variants: Option<Vec<String>> }
    struct ShapeDecl { name: String, cols: Vec<Col> }
    enum Item { ... Brand(BrandDecl), Shape(ShapeDecl), ... }
    struct RelDecl { ..., shape_ref: Option<String> }   // Default = None

    // lex.rs
    enum Tok { ..., Pipe }

    // parse.rs
    fn type_decl(&mut self) -> Result<Item>   // replaces the single brand_decl dispatch
    fn rel_decl(&mut self) -> Result<RelDecl>  // + `rel name : shape .` arm

    // typecheck.rs
    struct Brands { parent: HashMap<String,String>, variants: HashMap<String,Vec<String>> }
    fn Brands::enum_variants(&self, name: &str) -> Option<&[String]>   // walks parent chain
    fn expand_shapes(items: &mut Vec<Item>, dl_path: &str, diags: &mut Vec<TypeDiag>)
    fn enum_lit_check(brand, val, brands, col_name, dl_path, diags)
    fn nearest_variant(val: &str, variants: &[String]) -> Option<&str>  // min edit distance

## Pseudo-code (bodies)

    type_decl:
      ident("type"); name = ident()
      match peek():
        Lt2   => { next; parent = ident(); Dot; Brand{name, parent, variants:None} }
        Eq    => { next; v0 = str_lit(); vars=[v0]; while peek()==Pipe { next; vars.push(str_lit()) }
                   Dot; Brand{name, parent:"text", variants:Some(vars)} }
        LParen=> { cols = col_list(); Dot; Shape{name, cols} }
        other => bail "type `{name}`: expected `<:` (brand), `=` (enum), or `(` (shape)"

    rel_decl (after name):
      if peek()==Colon { next; shape = ident(); Dot; return RelDecl{name, shape_ref:Some(shape), ..default} }
      ... existing `(cols) qualifiers .` path ...

    expand_shapes(items, dl_path, diags):
      shapes = { s.name -> s.cols.clone() for Item::Shape }   // dup name -> error
      for Item::Rel d where d.shape_ref == Some(sname):
        match shapes.get(sname):
          Some(cols) => { d.cols = cols.clone(); d.shape_ref = None }
          None       => diag error "unknown-shape": "rel `{name}`: unknown shape `{sname}`
                        — declare `type {sname}(...)` or use `rel {name}(cols)`"
      items.retain(|i| !Item::Shape)

    Brands::from_program: enum brand inserts parent="text" + variants map entry.
    Brands::enum_variants(name): walk parent chain (bounded by len+1) returning the
      first brand carrying a variant set (so `type x <: severity` inherits).

    check_rule_types visit_atom, new arm after the Str-in-int arm:
      Term::Str(s) if cty.brand.is_some() => enum_lit_check(brand, s, ...)
    check_and_normalize: also loop Item::Query heads -> enum_lit_check on Str terms.

## Instance lifetimes

- `Brands` / shape table: transient, rebuilt each `check_and_normalize` call; nothing persists.
- `RelDecl.shape_ref`: lives only between parse and `expand_shapes`; None afterward.
- `Item::Shape`: removed by `expand_shapes` before validate_brands / prog_rels / engine.

## Storage / reads / writes

- No table, no SQL, no `_reldigest`. Columns remain TEXT (`Type::sql` unchanged).
- Reads: parse produces Brand/Shape/shape_ref; `expand_shapes` (frontend load +
  check_and_normalize) reads shapes, writes rel cols. `Brands` reads variants.
- Writes: only `TypeDiag`s (`enum-variant-unknown`, `unknown-shape`) into the diag
  channel that `--check` / `--parse-only` / LSP already render.
- Uniqueness: duplicate brand and duplicate shape names error; a shape col's brand
  ref is validated by the existing `validate_brands` AFTER expansion (shape cols
  become rel cols).

## Call order

expand_shapes runs in frontend `load_program`/`load_program_set` (so
`resolve_named_args` and `dedup_rels` see real cols) AND idempotently at the top of
`check_and_normalize` (covers the daemon `run_eval` snippet path that bypasses the
frontend). Enum checks run inside `check_and_normalize` after expansion.
