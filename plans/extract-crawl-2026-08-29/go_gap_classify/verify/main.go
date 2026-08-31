// argv: <corpusRoot> <classesTsv> <outTsv>
// classesTsv rows: srcPath srcName dstPath dstName class evidence where
// For package-qualified-call rows: split our dst against go/types' unique
// resolution (match = our row correct / mismatch = possible wrong-target).
// For concrete-one-hop-receiver rows: assert the callee method exists on the
// receiver's type. For rows whose dst lives in *_generated.go: assert the
// callee symbol is declared in that file.
package main

import (
	"bufio"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"golang.org/x/tools/go/packages"
)

type row struct {
	srcPath, srcName, dstPath, dstName, class, evidence, where string
}

type site struct {
	pos      ast.Expr // the selector expression node
	kind     string   // "qualified" (sel==nil) or "selection"
	obj      *types.Func
	recvType types.Type
	file     *ast.File
	pkg      *packages.Package
}

func main() {
	corpusRoot := os.Args[1]
	classesPath := os.Args[2]
	outPath := os.Args[3]

	rows := loadRows(classesPath)
	need := map[string]bool{}
	for _, r := range rows {
		need[r.srcPath+"\t"+r.srcName] = true
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

	// (srcPath, srcName) -> candidate sites that name the row's dstName.
	sites := map[string][]*site{}
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
				fn, ok := decl.(*ast.FuncDecl)
				if !ok || fn.Body == nil {
					continue
				}
				key := srcPath + "\t" + fn.Name.Name
				if !need[key] {
					continue
				}
				ast.Inspect(fn.Body, func(n ast.Node) bool {
					call, ok := n.(*ast.CallExpr)
					if !ok {
						return true
					}
					sel, ok := call.Fun.(*ast.SelectorExpr)
					if !ok {
						return true
					}
					info := pkg.TypesInfo
					s := &site{pos: sel, pkg: pkg, file: file}
					if info.Selections[sel] == nil {
						obj, _ := info.Uses[sel.Sel].(*types.Func)
						if obj == nil {
							return true
						}
						s.kind = "qualified"
						s.obj = obj
					} else {
						obj, _ := info.Selections[sel].Obj().(*types.Func)
						if obj == nil {
							return true
						}
						s.kind = "selection"
						s.obj = obj
						s.recvType = info.Selections[sel].Recv()
					}
					sites[key] = append(sites[key], s)
					return true
				})
			}
		}
	})

	out, err := os.Create(outPath)
	must(err)
	defer out.Close()
	w := bufio.NewWriter(out)
	defer w.Flush()

	counts := map[string]int{}
	examples := map[string][]string{}
	genCounts := map[string]int{}
	record := func(r row, verdict, detail string) {
		counts[verdict]++
		if len(examples[verdict]) < 5 {
			examples[verdict] = append(examples[verdict],
				fmt.Sprintf("%s %s -> %s %s [%s] %s", r.srcPath, r.srcName, r.dstPath, r.dstName, detail, r.where))
		}
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\t%s\n",
			r.srcPath, r.srcName, r.dstPath, r.dstName, r.class, verdict, detail)
	}

	for _, r := range rows {
		// Task 3: generated-file dst, any class.
		if strings.HasSuffix(r.dstPath, "_generated.go") && corpusFile(corpusRoot, r.dstPath) != "" {
			data, err := os.ReadFile(corpusFile(corpusRoot, r.dstPath))
			if err == nil {
				genRe := regexp.MustCompile(`(?m)^func (\([^)]*\) )?` + regexp.QuoteMeta(r.dstName) + `(\[|\()|^const ` + regexp.QuoteMeta(r.dstName) + `\b|^var ` + regexp.QuoteMeta(r.dstName) + `\b|^type ` + regexp.QuoteMeta(r.dstName) + `\b`)
				ok := genRe.Match(data)
				if ok {
					genCounts["gen-symbol-present"]++
				} else {
					genCounts["gen-symbol-ABSENT"]++
				}
				if len(examples["gen:"+map[bool]string{true: "present", false: "ABSENT"}[ok]]) < 5 {
					k := "gen:" + map[bool]string{true: "present", false: "ABSENT"}[ok]
					examples[k] = append(examples[k],
						fmt.Sprintf("%s %s -> %s %s", r.srcPath, r.srcName, r.dstPath, r.dstName))
				}
			}
		}
		if r.class != "package-qualified-call" && r.class != "concrete-one-hop-receiver" {
			continue
		}
		key := r.srcPath + "\t" + r.srcName
		target := r.dstPath + "\t" + r.dstName
		var cands []*site
		for _, s := range sites[key] {
			if s.obj.Name() == r.dstName {
				cands = append(cands, s)
			}
		}
		if len(cands) == 0 {
			record(r, "no-site", "no resolved call site names "+r.dstName)
			continue
		}
		if r.class == "package-qualified-call" {
			// Prefer the qualified site: `pkg.F(...)` is resolved by go/types
			// to the unique package-level object, so our dst either matches
			// (row correct, vta-side gap) or it does not (wrong-target).
			var q *site
			for _, s := range cands {
				if s.kind == "qualified" {
					q = s
					break
				}
			}
			if q == nil {
				record(r, "no-qualified-site", "only selection sites name "+r.dstName)
				continue
			}
			objKey := objKey(corpusRoot, q.pkg.Fset, q.obj)
			if objKey == target {
				record(r, "pq-match", "go/types resolves pkg."+r.dstName+" to exactly our dst ("+objKey+")")
			} else {
				record(r, "pq-MISMATCH", "go/types resolves pkg."+r.dstName+" to "+objKey+", ours says "+target)
			}
			continue
		}
		// concrete-one-hop-receiver: assert the method exists on the receiver
		// type and picks out our dst.
		for _, s := range cands {
			if s.kind != "selection" || s.recvType == nil {
				continue
			}
			obj, _, _ := types.LookupFieldOrMethod(s.recvType, true, s.pkg.Types, r.dstName)
			fn, _ := obj.(*types.Func)
			if fn == nil {
				record(r, "recv-ABSENT", "method "+r.dstName+" not found on "+typeString(s.recvType))
				continue
			}
			objKey := objKey(corpusRoot, s.pkg.Fset, fn)
			if objKey == target {
				record(r, "recv-match", "method exists on "+typeString(s.recvType)+" and binds exactly our dst")
				break
			}
			record(r, "recv-OTHER", "method exists on "+typeString(s.recvType)+" but binds "+objKey+", ours says "+target)
			break
		}
	}

	var verdicts []string
	for c := range counts {
		verdicts = append(verdicts, c)
	}
	for c := range genCounts {
		verdicts = append(verdicts, "GEN/"+c)
	}
	sort.Slice(verdicts, func(i, j int) bool { return counts[verdicts[i]]+genCounts[strings.TrimPrefix(verdicts[i], "GEN/")] > counts[verdicts[j]]+genCounts[strings.TrimPrefix(verdicts[j], "GEN/")] })
	for _, c := range verdicts {
		if strings.HasPrefix(c, "GEN/") {
			fmt.Printf("%6d  %s\n", genCounts[strings.TrimPrefix(c, "GEN/")], c)
			for _, ex := range examples["gen:"+strings.TrimPrefix(c, "GEN/")] {
				fmt.Printf("        %s\n", ex)
			}
			continue
		}
		fmt.Printf("%6d  %s\n", counts[c], c)
		for _, ex := range examples[c] {
			fmt.Printf("        %s\n", ex)
		}
	}
}

func corpusFile(root, rel string) string {
	p := filepath.Join(root, rel)
	if _, err := os.Stat(p); err != nil {
		return ""
	}
	return p
}

func objKey(corpusRoot string, fset *token.FileSet, obj *types.Func) string {
	if obj == nil || obj.Pos() == token.NoPos {
		return ""
	}
	pos := fset.Position(obj.Pos())
	if pos.Filename == "" || !strings.HasPrefix(pos.Filename, corpusRoot) {
		return ""
	}
	return relPath(corpusRoot, pos.Filename) + "\t" + obj.Name()
}

func typeString(t types.Type) string {
	return types.TypeString(t, func(p *types.Package) string { return p.Name() })
}

func loadRows(path string) []row {
	var out []row
	data, err := os.ReadFile(path)
	must(err)
	for _, line := range strings.Split(string(data), "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		c := strings.Split(line, "\t")
		if len(c) < 5 {
			continue
		}
		r := row{c[0], c[1], c[2], c[3], c[4], "", ""}
		if len(c) > 6 {
			r.evidence, r.where = c[5], c[6]
		}
		out = append(out, r)
	}
	return out
}

func relPath(root, abs string) string {
	rel, err := filepath.Rel(root, abs)
	if err != nil {
		return abs
	}
	return rel
}

func must(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
