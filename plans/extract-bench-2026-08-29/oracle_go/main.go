// Emits normal-form tsvs: src_path src_name dst_path dst_name.
// argv: <corpusRoot> <outDir> [family]   family in {all, type}, default all.
package main

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"golang.org/x/tools/go/callgraph"
	"golang.org/x/tools/go/callgraph/cha"
	"golang.org/x/tools/go/callgraph/vta"
	"golang.org/x/tools/go/packages"
	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
)

func main() {
	corpusRoot := os.Args[1]
	outDir := os.Args[2]
	family := "all"
	if len(os.Args) > 3 {
		family = os.Args[3]
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedDeps | packages.NeedTypes |
			packages.NeedTypesInfo | packages.NeedSyntax | packages.NeedModule,
		Dir: corpusRoot,
	}
	pkgs, err := packages.Load(cfg, "./...")
	must(err)
	fmt.Fprintf(os.Stderr, "loaded %d packages\n", len(pkgs))

	if family == "type" {
		writeTypeEdges(corpusRoot, outDir, pkgs)
		return
	}

	writeModuleEdges(corpusRoot, outDir, pkgs)
	writeTypeEdges(corpusRoot, outDir, pkgs)

	prog, ssaPkgs := ssautil.AllPackages(pkgs, ssa.InstantiateGenerics)
	prog.Build()
	fmt.Fprintf(os.Stderr, "built ssa for %d packages\n", len(ssaPkgs))

	allFuncs := ssautil.AllFunctions(prog)
	fmt.Fprintf(os.Stderr, "cha...\n")
	chaGraph := cha.CallGraph(prog)
	writeCallEdges(corpusRoot, filepath.Join(outDir, "go.oracle.call.cha.tsv"), chaGraph)

	fmt.Fprintf(os.Stderr, "vta...\n")
	vtaGraph := vta.CallGraph(allFuncs, chaGraph)
	writeCallEdges(corpusRoot, filepath.Join(outDir, "go.oracle.call.vta.tsv"), vtaGraph)
}

func must(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func relPath(root, abs string) string {
	rel, err := filepath.Rel(root, abs)
	if err != nil {
		return abs
	}
	return rel
}

func writeModuleEdges(corpusRoot, outDir string, pkgs []*packages.Package) {
	var rows []string
	seen := map[string]bool{}
	packages.Visit(pkgs, nil, func(pkg *packages.Package) {
		for i, file := range pkg.Syntax {
			if i >= len(pkg.CompiledGoFiles) {
				continue
			}
			srcPath := relPath(corpusRoot, pkg.CompiledGoFiles[i])
			for _, imp := range file.Imports {
				path := strings.Trim(imp.Path.Value, `"`)
				impPkg := pkg.Imports[path]
				if impPkg == nil || len(impPkg.GoFiles) == 0 {
					continue
				}
				dstPath := relPath(corpusRoot, filepath.Dir(impPkg.GoFiles[0]))
				if strings.HasPrefix(dstPath, "..") {
					continue
				}
				row := srcPath + "\t\t" + dstPath + "\t"
				if !seen[row] {
					seen[row] = true
					rows = append(rows, row)
				}
			}
		}
	})
	sort.Strings(rows)
	fmt.Fprintf(os.Stderr, "module edges: %d\n", len(rows))
	must(os.WriteFile(filepath.Join(outDir, "go.oracle.module.tsv"), []byte(strings.Join(rows, "\n")+"\n"), 0o644))
}

func funcFile(fset *token.FileSet, fn *ssa.Function) (string, bool) {
	if fn == nil || fn.Pkg == nil {
		return "", false
	}
	pos := fn.Pos()
	if pos == token.NoPos {
		if fn.Synthetic != "" {
			return "", false
		}
		return "", false
	}
	position := fset.Position(pos)
	if position.Filename == "" {
		return "", false
	}
	return position.Filename, true
}

func funcName(fn *ssa.Function) string {
	if fn.Signature != nil {
		if recv := fn.Signature.Recv(); recv != nil {
			t := recv.Type()
			if p, ok := t.(*types.Pointer); ok {
				t = p.Elem()
			}
			if named, ok := t.(*types.Named); ok {
				return named.Obj().Name() + "." + fn.Name()
			}
		}
	}
	return fn.Name()
}

func writeCallEdges(corpusRoot, outPath string, graph *callgraph.Graph) {
	var rows []string
	seen := map[string]bool{}
	for fn, node := range graph.Nodes {
		if fn == nil {
			continue
		}
		fset := fnFileSet(fn)
		srcFile, ok := funcFile(fset, fn)
		if !ok || !strings.HasPrefix(srcFile, corpusRoot) {
			continue
		}
		srcPath := relPath(corpusRoot, srcFile)
		srcName := funcName(fn)
		for _, edge := range node.Out {
			callee := edge.Callee.Func
			if callee == nil {
				continue
			}
			dstFset := fnFileSet(callee)
			dstFile, ok := funcFile(dstFset, callee)
			if !ok || !strings.HasPrefix(dstFile, corpusRoot) {
				continue
			}
			dstPath := relPath(corpusRoot, dstFile)
			dstName := funcName(callee)
			row := srcPath + "\t" + srcName + "\t" + dstPath + "\t" + dstName
			if !seen[row] {
				seen[row] = true
				rows = append(rows, row)
			}
		}
	}
	sort.Strings(rows)
	fmt.Fprintf(os.Stderr, "call edges in %s: %d\n", filepath.Base(outPath), len(rows))
	must(os.WriteFile(outPath, []byte(strings.Join(rows, "\n")+"\n"), 0o644))
}

func fnFileSet(fn *ssa.Function) *token.FileSet {
	if fn.Prog == nil {
		return nil
	}
	return fn.Prog.Fset
}

// rows: 5-col row -> owner is a type declaration (as opposed to a fn or a var).
type typeEdgeSink struct {
	corpusRoot string
	fset       *token.FileSet
	rows       map[string]bool
}

func (s *typeEdgeSink) objPath(obj types.Object) (string, bool) {
	if obj == nil || obj.Pos() == token.NoPos {
		return "", false
	}
	pos := s.fset.Position(obj.Pos())
	if pos.Filename == "" || !strings.HasPrefix(pos.Filename, s.corpusRoot) {
		return "", false
	}
	return relPath(s.corpusRoot, pos.Filename), true
}

func (s *typeEdgeSink) add(srcPath, srcName, dstPath, dstName, kind string, typeDeclOwner bool) {
	if srcName == "" || dstName == "" || srcPath == "" || dstPath == "" {
		return
	}
	row := strings.Join([]string{srcPath, srcName, dstPath, dstName, kind}, "\t")
	s.rows[row] = s.rows[row] || typeDeclOwner
}

// A named non-typeparam TypeName reached from an ident, or nothing.
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

func (s *typeEdgeSink) refsUnder(pkg *packages.Package, node ast.Node, srcPath, srcName string, typeDeclOwner bool) {
	ast.Inspect(node, func(n ast.Node) bool {
		ident, ok := n.(*ast.Ident)
		if !ok {
			return true
		}
		obj := pkg.TypesInfo.Uses[ident]
		tn, ok := obj.(*types.TypeName)
		if !ok {
			return true
		}
		if _, isParam := tn.Type().(*types.TypeParam); isParam {
			return true
		}
		dstPath, ok := s.objPath(tn)
		if !ok {
			return true
		}
		s.add(srcPath, srcName, dstPath, tn.Name(), "ref", typeDeclOwner)
		return true
	})
}

func declOwnerName(decl ast.Decl) string {
	fn, ok := decl.(*ast.FuncDecl)
	if !ok {
		return ""
	}
	return fn.Name.Name
}

func writeTypeEdges(corpusRoot, outDir string, pkgs []*packages.Package) {
	sink := &typeEdgeSink{corpusRoot: corpusRoot, fset: pkgs[0].Fset, rows: map[string]bool{}}

	var namedTypes []*types.Named
	var interfaces []*types.Named
	seenPkg := map[string]bool{}

	packages.Visit(pkgs, nil, func(pkg *packages.Package) {
		if pkg.TypesInfo == nil || seenPkg[pkg.PkgPath] {
			return
		}
		seenPkg[pkg.PkgPath] = true

		for _, file := range pkg.Syntax {
			pos := pkg.Fset.Position(file.Pos())
			if pos.Filename == "" || !strings.HasPrefix(pos.Filename, corpusRoot) {
				continue
			}
			srcPath := relPath(corpusRoot, pos.Filename)
			for _, decl := range file.Decls {
				switch d := decl.(type) {
				case *ast.FuncDecl:
					sink.refsUnder(pkg, d, srcPath, declOwnerName(d), false)
				case *ast.GenDecl:
					if d.Tok == token.IMPORT {
						continue
					}
					for _, spec := range d.Specs {
						switch sp := spec.(type) {
						case *ast.TypeSpec:
							sink.refsUnder(pkg, sp, srcPath, sp.Name.Name, true)
						case *ast.ValueSpec:
							if len(sp.Names) == 0 {
								continue
							}
							sink.refsUnder(pkg, sp, srcPath, sp.Names[0].Name, false)
						}
					}
				}
			}
		}

		if pkg.Types == nil {
			return
		}
		scope := pkg.Types.Scope()
		for _, name := range scope.Names() {
			tn, ok := scope.Lookup(name).(*types.TypeName)
			if !ok {
				continue
			}
			named, ok := tn.Type().(*types.Named)
			if !ok || named.TypeParams().Len() > 0 {
				continue
			}
			if _, ok := sink.objPath(tn); !ok {
				continue
			}
			namedTypes = append(namedTypes, named)
			if iface, ok := named.Underlying().(*types.Interface); ok && iface.NumMethods() > 0 {
				interfaces = append(interfaces, named)
			}
		}
	})

	fmt.Fprintf(os.Stderr, "named types: %d, interfaces with >=1 method: %d, pairs: %d\n",
		len(namedTypes), len(interfaces), len(namedTypes)*len(interfaces))

	implements := 0
	for _, concrete := range namedTypes {
		srcPath, ok := sink.objPath(concrete.Obj())
		if !ok {
			continue
		}
		pointer := types.NewPointer(concrete)
		for _, iface := range interfaces {
			if concrete == iface {
				continue
			}
			under := iface.Underlying().(*types.Interface)
			if !types.Implements(concrete, under) && !types.Implements(pointer, under) {
				continue
			}
			dstPath, ok := sink.objPath(iface.Obj())
			if !ok {
				continue
			}
			sink.add(srcPath, concrete.Obj().Name(), dstPath, iface.Obj().Name(), "implements", true)
			implements++
		}
	}

	extends := 0
	for _, named := range namedTypes {
		strct, ok := named.Underlying().(*types.Struct)
		if !ok {
			continue
		}
		srcPath, ok := sink.objPath(named.Obj())
		if !ok {
			continue
		}
		for i := 0; i < strct.NumFields(); i++ {
			field := strct.Field(i)
			if !field.Anonymous() {
				continue
			}
			tn := namedTypeName(field.Type())
			if tn == nil {
				continue
			}
			dstPath, ok := sink.objPath(tn)
			if !ok {
				continue
			}
			sink.add(srcPath, named.Obj().Name(), dstPath, tn.Name(), "extends", true)
			extends++
		}
	}

	kinded := make([]string, 0, len(sink.rows))
	bare := map[string]bool{}
	typeDecl := map[string]bool{}
	for row, ownedByTypeDecl := range sink.rows {
		kinded = append(kinded, row)
		stripped := row[:strings.LastIndex(row, "\t")]
		bare[stripped] = true
		if ownedByTypeDecl {
			typeDecl[stripped] = true
		}
	}
	sort.Strings(kinded)

	fmt.Fprintf(os.Stderr, "type edges: kinded=%d bare=%d typedecl=%d implements=%d extends=%d\n",
		len(kinded), len(bare), len(typeDecl), implements, extends)
	writeSorted(filepath.Join(outDir, "go.oracle.type.tsv"), bare)
	writeSorted(filepath.Join(outDir, "go.oracle.type.typedecl.tsv"), typeDecl)
	must(os.WriteFile(filepath.Join(outDir, "go.oracle.type.kinds.tsv"), []byte(strings.Join(kinded, "\n")+"\n"), 0o644))
}

func writeSorted(outPath string, set map[string]bool) {
	rows := make([]string, 0, len(set))
	for row := range set {
		rows = append(rows, row)
	}
	sort.Strings(rows)
	must(os.WriteFile(outPath, []byte(strings.Join(rows, "\n")+"\n"), 0o644))
}
