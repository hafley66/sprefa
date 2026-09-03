// The go CHECKER tier's sidecar: go/packages loads the project, go/types
// answers the destination of every call and type reference the caller names,
// and each answer leaves here as one JSON line on stdout.
//
// argv: <request.json>. The request carries the project root, the supplied
// path and absolute path of every file the caller wants answered, and whether
// the type-relation walk runs.
package main

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"sort"
	"time"

	"golang.org/x/tools/go/packages"
)

type request struct {
	Root  string      `json:"root"`
	Files [][2]string `json:"files"`
	Tsi   bool        `json:"tsi"`
}

// [start, end, name, dstPath, dstName, dstOffset]. An empty dstPath is the
// checker saying the destination sits outside the supplied file set, which is
// knowledge rather than absence.
type wireRow struct {
	start     int
	end       int
	name      string
	dstPath   string
	dstName   string
	dstOffset int
}

func (r wireRow) MarshalJSON() ([]byte, error) {
	return json.Marshal([]any{r.start, r.end, r.name, r.dstPath, r.dstName, r.dstOffset})
}

type wireFile struct {
	Path  string    `json:"path"`
	Calls []wireRow `json:"calls"`
	Types []wireRow `json:"types"`
	Tsi   [][]any   `json:"tsi,omitempty"`
}

type wireCosts struct {
	LoadMs uint64 `json:"loadMs"`
	WalkMs uint64 `json:"walkMs"`
	Files  int    `json:"files"`
}

type wireStats struct {
	Stats    wireCosts `json:"stats"`
	Coverage [][3]any  `json:"coverage"`
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: go_checker <request.json>")
		os.Exit(2)
	}
	body, err := os.ReadFile(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	var req request
	if err := json.Unmarshal(body, &req); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	loadStart := time.Now()
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedDeps | packages.NeedTypes |
			packages.NeedTypesInfo | packages.NeedSyntax | packages.NeedModule,
		Dir: req.Root,
	}
	pkgs, err := packages.Load(cfg, "./...")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	loadMs := uint64(time.Since(loadStart).Milliseconds())
	if len(pkgs) == 0 {
		fmt.Fprintf(os.Stderr, "go/packages loaded no package under %s\n", req.Root)
		os.Exit(1)
	}

	// Supplied path per absolute path: the answer wire speaks the caller's
	// spelling, never this process's view of the filesystem.
	supplied := map[string]string{}
	for _, pair := range req.Files {
		abs, err := filepath.EvalSymlinks(pair[1])
		if err != nil {
			abs = pair[1]
		}
		supplied[abs] = pair[0]
		supplied[pair[1]] = pair[0]
	}

	walkStart := time.Now()
	sink := &walkSink{supplied: supplied, tsi: req.Tsi, types: map[string]uint32{}}
	seen := map[string]bool{}
	packages.Visit(pkgs, nil, func(pkg *packages.Package) {
		if pkg.TypesInfo == nil || seen[pkg.PkgPath] {
			return
		}
		seen[pkg.PkgPath] = true
		sink.pkgs = append(sink.pkgs, pkg)
		for _, file := range pkg.Syntax {
			position := pkg.Fset.Position(file.Pos())
			path, wanted := sink.path(pkg.Fset, file.Pos())
			if !wanted || position.Filename == "" {
				continue
			}
			sink.file(pkg, file, path)
		}
	})
	if req.Tsi {
		sink.semantic()
	}
	walkMs := uint64(time.Since(walkStart).Milliseconds())

	encoder := json.NewEncoder(os.Stdout)
	answered := 0
	for _, path := range sink.order {
		rows := sink.files[path]
		answered++
		if err := encoder.Encode(wireFile{
			Path:  path,
			Calls: rows.calls,
			Types: rows.types,
			Tsi:   rows.tsi,
		}); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
	}
	coverage := [][3]any{}
	if req.Tsi {
		// The walk enumerates the loaded packages' own declarations, so no
		// relation it emits is complete over the whole program: a type
		// declared in a dependency is reachable and never enumerated.
		for _, relation := range sink.relations() {
			coverage = append(coverage, [3]any{
				relation,
				false,
				"go/types enumerated the loaded packages' declarations; dependency and stdlib declarations are reachable and not walked",
			})
		}
	}
	if err := encoder.Encode(wireStats{
		Stats:    wireCosts{LoadMs: loadMs, WalkMs: walkMs, Files: answered},
		Coverage: coverage,
	}); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

type fileRows struct {
	calls []wireRow
	types []wireRow
	tsi   [][]any
}

type walkSink struct {
	supplied map[string]string
	tsi      bool
	pkgs     []*packages.Package
	files    map[string]*fileRows
	order    []string

	// Run-local ids, minted per distinct type key and per distinct symbol.
	next    uint32
	types   map[string]uint32
	rows    [][]any
	named   []*types.Named
	ifaces  []*types.Named
	seenRel map[string]bool
}

// The supplied path of the file `pos` sits in, and whether the caller asked
// for it at all.
func (s *walkSink) path(fset *token.FileSet, pos token.Pos) (string, bool) {
	if pos == token.NoPos {
		return "", false
	}
	name := fset.Position(pos).Filename
	if name == "" {
		return "", false
	}
	if path, ok := s.supplied[name]; ok {
		return path, true
	}
	if resolved, err := filepath.EvalSymlinks(name); err == nil {
		if path, ok := s.supplied[resolved]; ok {
			return path, true
		}
	}
	return "", false
}

func (s *walkSink) rowsFor(path string) *fileRows {
	if s.files == nil {
		s.files = map[string]*fileRows{}
	}
	rows, ok := s.files[path]
	if !ok {
		rows = &fileRows{}
		s.files[path] = rows
		s.order = append(s.order, path)
	}
	return rows
}

// A method carries its receiver type in the name the parse mints for its def,
// so the answer wire spells it the same way.
func objName(obj types.Object) string {
	fn, ok := obj.(*types.Func)
	if !ok {
		return obj.Name()
	}
	signature, ok := fn.Type().(*types.Signature)
	if !ok || signature.Recv() == nil {
		return fn.Name()
	}
	recv := signature.Recv().Type()
	if pointer, ok := recv.(*types.Pointer); ok {
		recv = pointer.Elem()
	}
	if named, ok := recv.(*types.Named); ok {
		return named.Obj().Name() + "." + fn.Name()
	}
	return fn.Name()
}

// The destination coordinate of one resolved object: supplied path, the name
// the parse minted, the declaring identifier's byte offset. An object whose
// declaration sits outside the supplied set answers the empty path, which the
// caller reads as External.
func (s *walkSink) destination(fset *token.FileSet, obj types.Object) (string, string, int) {
	if obj == nil || obj.Pos() == token.NoPos {
		return "", "", 0
	}
	path, wanted := s.path(fset, obj.Pos())
	if !wanted {
		return "", "", 0
	}
	return path, objName(obj), fset.Position(obj.Pos()).Offset
}

// The rightmost identifier of a callee expression: `a.b.c(x)` names `c`.
func calleeIdent(fun ast.Expr) *ast.Ident {
	switch node := fun.(type) {
	case *ast.Ident:
		return node
	case *ast.SelectorExpr:
		return node.Sel
	case *ast.IndexExpr:
		return calleeIdent(node.X)
	case *ast.IndexListExpr:
		return calleeIdent(node.X)
	case *ast.ParenExpr:
		return calleeIdent(node.X)
	}
	return nil
}

func (s *walkSink) file(pkg *packages.Package, file *ast.File, path string) {
	rows := s.rowsFor(path)
	fset := pkg.Fset
	// The type plane keys on the name AS WRITTEN, so one row per distinct
	// spelling per file is all the caller can join on.
	typeSeen := map[string]bool{}
	ast.Inspect(file, func(node ast.Node) bool {
		switch n := node.(type) {
		case *ast.CallExpr:
			ident := calleeIdent(n.Fun)
			if ident == nil {
				return true
			}
			obj := pkg.TypesInfo.Uses[ident]
			if obj == nil {
				obj = pkg.TypesInfo.Defs[ident]
			}
			if _, isFunc := obj.(*types.Func); !isFunc {
				// A conversion `T(x)` or a builtin is not a call edge the
				// resolve plane carries; the syntax leg keeps whatever it made
				// of it.
				return true
			}
			dstPath, dstName, dstOffset := s.destination(fset, obj)
			start := fset.Position(ident.Pos()).Offset
			end := fset.Position(ident.End()).Offset
			rows.calls = append(rows.calls, wireRow{
				start:     start,
				end:       end,
				name:      ident.Name,
				dstPath:   dstPath,
				dstName:   dstName,
				dstOffset: dstOffset,
			})
		case *ast.Ident:
			typeName, ok := pkg.TypesInfo.Uses[n].(*types.TypeName)
			if !ok {
				return true
			}
			if _, isParam := typeName.Type().(*types.TypeParam); isParam {
				return true
			}
			if typeSeen[n.Name] {
				return true
			}
			typeSeen[n.Name] = true
			dstPath, dstName, dstOffset := s.destination(fset, typeName)
			rows.types = append(rows.types, wireRow{
				start:     fset.Position(n.Pos()).Offset,
				end:       fset.Position(n.End()).Offset,
				name:      n.Name,
				dstPath:   dstPath,
				dstName:   dstName,
				dstOffset: dstOffset,
			})
		}
		return true
	})
}

// The id for one type, minted once per distinct go/types key.
func (s *walkSink) id(key string) (uint32, bool) {
	if id, ok := s.types[key]; ok {
		return id, false
	}
	id := s.next
	s.next++
	s.types[key] = id
	return id, true
}

func (s *walkSink) add(path string, row []any) {
	s.rowsFor(path).tsi = append(s.rowsFor(path).tsi, row)
	if s.seenRel == nil {
		s.seenRel = map[string]bool{}
	}
	if name, ok := row[0].(string); ok {
		s.seenRel[name] = true
	}
}

func (s *walkSink) relations() []string {
	named := make([]string, 0, len(s.seenRel))
	for relation := range s.seenRel {
		named = append(named, relation)
	}
	sort.Strings(named)
	return named
}

func spanArg(path string, start, end int) map[string]any {
	return map[string]any{"span": []any{path, start, end}}
}

func idArg(id uint32) map[string]any  { return map[string]any{"id": id} }
func textArg(s string) map[string]any { return map[string]any{"text": s} }
func atomArg(s string) map[string]any { return map[string]any{"atom": s} }

// The TSI semantic block: every named declaration in the loaded packages that
// sits in a supplied file gets a type id, its checker spelling, its origin,
// and its shape; `types.Implements` then answers `tsi.conforms` over the
// interfaces among them.
func (s *walkSink) semantic() {
	// The declaring row must precede every row naming its id, so the whole
	// named set is minted before any relation over pairs runs.
	type decl struct {
		named *types.Named
		path  string
		start int
		end   int
		id    uint32
	}
	var decls []decl
	for _, pkg := range s.pkgs {
		if pkg.Types == nil {
			continue
		}
		scope := pkg.Types.Scope()
		for _, name := range scope.Names() {
			typeName, ok := scope.Lookup(name).(*types.TypeName)
			if !ok {
				continue
			}
			named, ok := typeName.Type().(*types.Named)
			if !ok || named.TypeParams().Len() > 0 {
				continue
			}
			path, wanted := s.path(pkg.Fset, typeName.Pos())
			if !wanted {
				continue
			}
			id, fresh := s.id(named.String())
			if !fresh {
				continue
			}
			position := pkg.Fset.Position(typeName.Pos())
			decls = append(decls, decl{
				named: named,
				path:  path,
				start: position.Offset,
				end:   position.Offset + len(typeName.Name()),
				id:    id,
			})
		}
	}
	sort.Slice(decls, func(i, j int) bool { return decls[i].id < decls[j].id })
	for _, d := range decls {
		s.add(d.path, []any{"tsi.type", idArg(d.id)})
		s.add(d.path, []any{"tsi.name", idArg(d.id), textArg(d.named.Obj().Name())})
		s.add(d.path, []any{"tsi.origin", idArg(d.id), atomArg("go"), spanArg(d.path, d.start, d.end)})
		s.add(d.path, []any{"tsi.symbol", idArg(d.id)})
		s.add(d.path, []any{"tsi.denotes", idArg(d.id), idArg(d.id)})
		switch d.named.Underlying().(type) {
		case *types.Struct:
			s.add(d.path, []any{"tsi.product", idArg(d.id)})
		case *types.Interface:
			s.add(d.path, []any{"tsi.sum", idArg(d.id)})
		case *types.Signature:
			s.add(d.path, []any{"tsi.callable", idArg(d.id)})
		case *types.Basic:
			s.add(d.path, []any{"tsi.primitive", idArg(d.id), atomArg(basicClass(d.named.Underlying()))})
		}
	}
	for _, concrete := range decls {
		pointer := types.NewPointer(concrete.named)
		for _, candidate := range decls {
			iface, ok := candidate.named.Underlying().(*types.Interface)
			if !ok || iface.NumMethods() == 0 || concrete.id == candidate.id {
				continue
			}
			if !types.Implements(concrete.named, iface) && !types.Implements(pointer, iface) {
				continue
			}
			s.add(concrete.path, []any{"tsi.conforms", idArg(concrete.id), idArg(candidate.id), atomArg("checker")})
			s.add(concrete.path, []any{"tsi.subtype", idArg(concrete.id), idArg(candidate.id), atomArg("checker")})
		}
	}
	for _, d := range decls {
		strct, ok := d.named.Underlying().(*types.Struct)
		if !ok {
			continue
		}
		for i := 0; i < strct.NumFields(); i++ {
			field := strct.Field(i)
			target := namedTypeName(field.Type())
			if target == nil {
				continue
			}
			id, ok := s.types[target.Type().String()]
			if !ok {
				continue
			}
			edge, _ := s.id(fmt.Sprintf("edge:%s:%s", d.named.String(), field.Name()))
			s.add(d.path, []any{"tsi.edge", idArg(edge), idArg(d.id), textArg(field.Name()), idArg(id), map[string]any{"int": i}})
		}
	}
}

func basicClass(t types.Type) string {
	basic, ok := t.(*types.Basic)
	if !ok {
		return "other"
	}
	switch {
	case basic.Info()&types.IsBoolean != 0:
		return "boolean"
	case basic.Info()&types.IsInteger != 0, basic.Info()&types.IsFloat != 0, basic.Info()&types.IsComplex != 0:
		return "number"
	case basic.Info()&types.IsString != 0:
		return "string"
	}
	return "other"
}

// A named non-typeparam TypeName reached from a type, or nothing.
func namedTypeName(t types.Type) *types.TypeName {
	switch u := t.(type) {
	case *types.Pointer:
		return namedTypeName(u.Elem())
	case *types.Named:
		return u.Obj()
	case *types.Alias:
		return u.Obj()
	}
	return nil
}
