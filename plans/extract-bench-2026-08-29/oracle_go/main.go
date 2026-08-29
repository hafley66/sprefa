// go/callgraph oracle over a Go module: packages.Load + ssa build, then
// cha and vta callgraphs. Emits normal-form tsvs: src_path src_name dst_path dst_name.
package main

import (
	"fmt"
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

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedDeps | packages.NeedTypes |
			packages.NeedTypesInfo | packages.NeedSyntax | packages.NeedModule,
		Dir: corpusRoot,
	}
	pkgs, err := packages.Load(cfg, "./...")
	must(err)
	fmt.Fprintf(os.Stderr, "loaded %d packages\n", len(pkgs))

	writeModuleEdges(corpusRoot, outDir, pkgs)

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
