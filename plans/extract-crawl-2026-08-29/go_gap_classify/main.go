// argv: <corpusRoot> <missedTsv> <outTsv>; missedTsv rows are the 4-column
// normal form, outTsv adds class, evidence and one caller site file:line.
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

	"golang.org/x/tools/go/packages"
)

// One syntactic call/selection site inside a named caller.
type site struct {
	pos       token.Position
	selName   string
	calleeKey string // dstPath \t dstName when the selection names a corpus func
	class     string
	evidence  string
}

type rowKey struct{ srcPath, srcName, dstPath, dstName string }

func main() {
	corpusRoot := os.Args[1]
	missedPath := os.Args[2]
	outPath := os.Args[3]

	missed, order := loadMissed(missedPath)

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedDeps | packages.NeedTypes |
			packages.NeedTypesInfo | packages.NeedSyntax | packages.NeedModule,
		Dir: corpusRoot,
	}
	pkgs, err := packages.Load(cfg, "./...")
	must(err)
	fmt.Fprintf(os.Stderr, "loaded %d packages\n", len(pkgs))

	// Implementer counts per interface NAME, so an interface-dispatch row can
	// say whether our fan-out cap (64) is what declined it.
	implCount := implementerCounts(pkgs)

	// (srcPath, srcName) -> every site inside that caller.
	byCaller := map[[2]string][]site{}
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
			origins := declOrigins(pkg, file)
			for _, decl := range file.Decls {
				fn, ok := decl.(*ast.FuncDecl)
				if !ok || fn.Body == nil {
					continue
				}
				key := [2]string{srcPath, fn.Name.Name}
				byCaller[key] = append(byCaller[key],
					sitesIn(corpusRoot, pkg, fn.Body, implCount, origins)...)
			}
		}
	})

	out, err := os.Create(outPath)
	must(err)
	defer out.Close()

	counts := map[string]int{}
	examples := map[string][]string{}
	for _, key := range order {
		if !missed[key] {
			continue
		}
		class, evidence, where := classify(byCaller, key)
		counts[class]++
		row := strings.Join([]string{
			key.srcPath, key.srcName, key.dstPath, key.dstName, class, evidence, where,
		}, "\t")
		fmt.Fprintln(out, row)
		if len(examples[class]) < 4 {
			examples[class] = append(examples[class],
				fmt.Sprintf("%s %s -> %s %s [%s] %s",
					key.srcPath, key.srcName, key.dstPath, key.dstName, evidence, where))
		}
	}

	var classes []string
	for c := range counts {
		classes = append(classes, c)
	}
	sort.Slice(classes, func(i, j int) bool { return counts[classes[i]] > counts[classes[j]] })
	for _, c := range classes {
		fmt.Printf("%6d  %s\n", counts[c], c)
		for _, ex := range examples[c] {
			fmt.Printf("        %s\n", ex)
		}
	}
}

// The class of one missed row: the strongest binding evidence among the sites
// in the caller that could produce it.
func classify(byCaller map[[2]string][]site, key rowKey) (string, string, string) {
	sites := byCaller[[2]string{key.srcPath, key.srcName}]
	target := key.dstPath + "\t" + key.dstName
	// Rank: a site whose resolved callee IS the row's target beats a site that
	// merely spells the name, which beats no site at all.
	var exact, byName []site
	for _, s := range sites {
		if s.calleeKey == target {
			exact = append(exact, s)
		} else if s.selName == key.dstName {
			byName = append(byName, s)
		}
	}
	pick := func(cands []site) (string, string, string) {
		best := cands[0]
		for _, s := range cands[1:] {
			if rank(s.class) > rank(best.class) {
				best = s
			}
		}
		return best.class, best.evidence, fmt.Sprintf("%s:%d", best.pos.Filename, best.pos.Line)
	}
	if len(exact) > 0 {
		return pick(exact)
	}
	if len(byName) > 0 {
		return pick(byName)
	}
	return "no-syntactic-site", "callee name absent from caller body", ""
}

// Which class wins when one caller holds several sites for the same target:
// a plain concrete site would already have resolved, so it ranks last.
func rank(class string) int {
	switch class {
	case "interface-dispatch-fanout-capped":
		return 7
	case "interface-dispatch":
		return 6
	case "embedded-promoted-method":
		return 5
	case "func-typed-field-or-value":
		return 4
	case "method-value":
		return 3
	case "generic-instantiation":
		return 2
	case "alias-receiver":
		return 2
	case "multi-hop-receiver":
		return 1
	}
	return 0
}

// Every call and every method value inside one function body, classified.
func sitesIn(corpusRoot string, pkg *packages.Package, body *ast.BlockStmt,
	implCount map[string]int, origins map[types.Object]string) []site {
	var out []site
	info := pkg.TypesInfo
	// A selector in call position is a call site; the same selector standing
	// alone is a method value, which binds an edge the oracle keeps.
	calledSelectors := map[ast.Expr]bool{}
	ast.Inspect(body, func(n ast.Node) bool {
		if call, ok := n.(*ast.CallExpr); ok {
			calledSelectors[unwrapIndex(call.Fun)] = true
		}
		return true
	})

	ast.Inspect(body, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.CallExpr:
			if s, ok := callSite(corpusRoot, pkg, node, implCount, origins); ok {
				out = append(out, s)
			}
		case *ast.SelectorExpr:
			if calledSelectors[ast.Expr(node)] {
				return true
			}
			sel := info.Selections[node]
			if sel == nil || sel.Kind() != types.MethodVal {
				return true
			}
			fn, ok := sel.Obj().(*types.Func)
			if !ok {
				return true
			}
			out = append(out, site{
				pos:       pkg.Fset.Position(node.Sel.Pos()),
				selName:   node.Sel.Name,
				calleeKey: objKey(corpusRoot, pkg.Fset, fn),
				class:     "method-value",
				evidence: fmt.Sprintf("%s taken as a value, not called (recv %s)",
					node.Sel.Name, typeString(sel.Recv())),
			})
		}
		return true
	})
	return out
}

func callSite(corpusRoot string, pkg *packages.Package, call *ast.CallExpr,
	implCount map[string]int, origins map[types.Object]string) (site, bool) {
	info := pkg.TypesInfo
	fun := unwrapIndex(call.Fun)
	generic := fun != call.Fun

	switch f := fun.(type) {
	case *ast.SelectorExpr:
		pos := pkg.Fset.Position(f.Sel.Pos())
		sel := info.Selections[f]
		if sel == nil {
			// Package-qualified call: `pkg.F(...)`, bound by the import.
			obj, _ := info.Uses[f.Sel].(*types.Func)
			class := "package-qualified-call"
			if generic {
				class = "generic-instantiation"
			}
			return site{
				pos:       pos,
				selName:   f.Sel.Name,
				calleeKey: objKey(corpusRoot, pkg.Fset, obj),
				class:     class,
				evidence:  "qualified by an import",
			}, true
		}
		recv := typeString(sel.Recv())
		promoted := len(sel.Index()) > 1
		switch sel.Kind() {
		case types.FieldVal:
			return site{
				pos: pos, selName: f.Sel.Name,
				class:    "func-typed-field-or-value",
				evidence: fmt.Sprintf("field %s.%s holds a func value", recv, f.Sel.Name),
			}, true
		case types.MethodExpr:
			fn, _ := sel.Obj().(*types.Func)
			return site{
				pos: pos, selName: f.Sel.Name,
				calleeKey: objKey(corpusRoot, pkg.Fset, fn),
				class:     "method-expression",
				evidence:  fmt.Sprintf("method expression on %s", recv),
			}, true
		}
		fn, ok := sel.Obj().(*types.Func)
		if !ok {
			return site{}, false
		}
		key := objKey(corpusRoot, pkg.Fset, fn)
		if isInterface(sel.Recv()) {
			bare := bareName(sel.Recv())
			class := "interface-dispatch"
			if implCount[bare] > 64 {
				class = "interface-dispatch-fanout-capped"
			}
			return site{
				pos: pos, selName: f.Sel.Name, calleeKey: key, class: class,
				evidence: fmt.Sprintf("receiver %s is an interface, %d implementers",
					recv, implCount[bare]),
			}, true
		}
		if promoted {
			return site{
				pos: pos, selName: f.Sel.Name, calleeKey: key,
				class: "embedded-promoted-method",
				evidence: fmt.Sprintf("%s promotes %s through %d embedded field(s), recv %s",
					recv, f.Sel.Name, len(sel.Index())-1, originOf(pkg, origins, f.X)),
			}, true
		}
		if generic {
			return site{
				pos: pos, selName: f.Sel.Name, calleeKey: key,
				class: "generic-instantiation", evidence: "explicit type arguments",
			}, true
		}
		class := "concrete-one-hop-receiver"
		if hops := receiverHops(f.X); hops > 1 {
			class = "multi-hop-receiver"
		}
		if under := aliasOf(sel.Recv()); under != "" {
			return site{
				pos: pos, selName: f.Sel.Name, calleeKey: key,
				class: "alias-receiver",
				evidence: fmt.Sprintf("receiver %s is an alias of %s, %d hop(s), recv %s",
					recv, under, receiverHops(f.X), originOf(pkg, origins, f.X)),
			}, true
		}
		return site{
			pos: pos, selName: f.Sel.Name, calleeKey: key, class: class,
			evidence: fmt.Sprintf("receiver %s, %d hop(s), recv %s",
				recv, receiverHops(f.X), originOf(pkg, origins, f.X)),
		}, true

	case *ast.Ident:
		pos := pkg.Fset.Position(f.Pos())
		switch obj := info.Uses[f].(type) {
		case *types.Func:
			class := "package-level-call"
			if generic {
				class = "generic-instantiation"
			}
			return site{
				pos: pos, selName: f.Name,
				calleeKey: objKey(corpusRoot, pkg.Fset, obj),
				class:     class, evidence: "bare name in this package",
			}, true
		case *types.Var:
			return site{
				pos: pos, selName: f.Name, class: "func-typed-field-or-value",
				evidence: fmt.Sprintf("local or parameter %s holds a func value", f.Name),
			}, true
		}
	}
	return site{}, false
}

// Where a receiver identifier's type comes from; anything but `param`,
// `var-typed` and `define:composite-literal` is outside our seeding legs.
func originOf(pkg *packages.Package, origins map[types.Object]string, expr ast.Expr) string {
	ident, ok := expr.(*ast.Ident)
	if !ok {
		return "expr:" + fmt.Sprintf("%T", expr)
	}
	obj := pkg.TypesInfo.Uses[ident]
	if obj == nil {
		obj = pkg.TypesInfo.Defs[ident]
	}
	if origin, ok := origins[obj]; ok {
		return origin
	}
	return "unknown"
}

// Every identifier this file DEFINES, mapped to the syntax that gives it a
// type. `types.Object` identity is the join key, so a shadowed name is safe.
func declOrigins(pkg *packages.Package, file *ast.File) map[types.Object]string {
	out := map[types.Object]string{}
	mark := func(name ast.Expr, origin string) {
		ident, ok := name.(*ast.Ident)
		if !ok {
			return
		}
		if obj := pkg.TypesInfo.Defs[ident]; obj != nil {
			out[obj] = origin
		}
	}
	markFields := func(list *ast.FieldList, origin string) {
		if list == nil {
			return
		}
		for _, field := range list.List {
			for _, name := range field.Names {
				mark(name, origin)
			}
		}
	}
	ast.Inspect(file, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.FuncDecl:
			markFields(node.Recv, "param")
			markFields(node.Type.Params, "param")
			markFields(node.Type.Results, "named-result")
		case *ast.FuncLit:
			markFields(node.Type.Params, "param")
			markFields(node.Type.Results, "named-result")
		case *ast.ValueSpec:
			origin := "var-inferred"
			if node.Type != nil {
				origin = "var-typed"
			}
			for _, name := range node.Names {
				mark(name, origin)
			}
		case *ast.AssignStmt:
			if node.Tok != token.DEFINE {
				return true
			}
			for i, lhs := range node.Lhs {
				origin := "define:multi-value"
				if len(node.Rhs) == len(node.Lhs) {
					origin = "define:" + rhsShape(node.Rhs[i])
				}
				mark(lhs, origin)
			}
		case *ast.RangeStmt:
			mark(node.Key, "range")
			mark(node.Value, "range")
		case *ast.TypeSwitchStmt:
			ast.Inspect(node.Assign, func(inner ast.Node) bool {
				if ident, ok := inner.(*ast.Ident); ok {
					mark(ident, "type-switch")
				}
				return true
			})
		}
		return true
	})
	return out
}

func rhsShape(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.CompositeLit:
		return "composite-literal"
	case *ast.CallExpr:
		return "call"
	case *ast.Ident:
		return "ident"
	case *ast.SelectorExpr:
		return "field-or-qualified"
	case *ast.IndexExpr, *ast.IndexListExpr:
		return "index"
	case *ast.TypeAssertExpr:
		return "type-assert"
	case *ast.FuncLit:
		return "func-literal"
	case *ast.UnaryExpr:
		return "unary-" + rhsShape(e.X)
	case *ast.StarExpr:
		return "deref"
	case *ast.ParenExpr:
		return rhsShape(e.X)
	}
	return "other"
}

// Selector/call/index steps in a receiver expression: `x` is 1, `x.f` is 2,
// and anything above 1 is a chain our one-hop bind plan does not type.
func receiverHops(expr ast.Expr) int {
	switch e := expr.(type) {
	case *ast.Ident:
		return 1
	case *ast.SelectorExpr:
		return receiverHops(e.X) + 1
	case *ast.CallExpr:
		return receiverHops(e.Fun) + 1
	case *ast.IndexExpr:
		return receiverHops(e.X) + 1
	case *ast.ParenExpr:
		return receiverHops(e.X)
	case *ast.StarExpr:
		return receiverHops(e.X)
	case *ast.TypeAssertExpr:
		return receiverHops(e.X) + 1
	}
	return 9
}

func unwrapIndex(expr ast.Expr) ast.Expr {
	switch e := expr.(type) {
	case *ast.IndexExpr:
		return e.X
	case *ast.IndexListExpr:
		return e.X
	case *ast.ParenExpr:
		return unwrapIndex(e.X)
	}
	return expr
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

// The type an alias receiver really names; "" when the receiver is not one.
func aliasOf(t types.Type) string {
	if p, ok := t.(*types.Pointer); ok {
		t = p.Elem()
	}
	if alias, ok := t.(*types.Alias); ok {
		return types.Unalias(alias).String()
	}
	return ""
}

func isInterface(t types.Type) bool {
	if p, ok := t.(*types.Pointer); ok {
		t = p.Elem()
	}
	return types.IsInterface(t)
}

func bareName(t types.Type) string {
	if p, ok := t.(*types.Pointer); ok {
		t = p.Elem()
	}
	switch n := t.(type) {
	case *types.Named:
		return n.Obj().Name()
	case *types.Alias:
		return n.Obj().Name()
	}
	return types.TypeString(t, nil)
}

func typeString(t types.Type) string {
	return types.TypeString(t, func(p *types.Package) string { return p.Name() })
}

// Named type -> how many named corpus types satisfy it, by interface NAME:
// the key our fan-out cap is keyed on.
func implementerCounts(pkgs []*packages.Package) map[string]int {
	var ifaces []*types.Named
	var concrete []types.Type
	seen := map[string]bool{}
	packages.Visit(pkgs, nil, func(pkg *packages.Package) {
		if pkg.Types == nil || seen[pkg.PkgPath] {
			return
		}
		seen[pkg.PkgPath] = true
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
			if iface, ok := named.Underlying().(*types.Interface); ok && iface.NumMethods() > 0 {
				ifaces = append(ifaces, named)
				continue
			}
			concrete = append(concrete, named, types.NewPointer(named))
		}
	})
	counts := map[string]int{}
	for _, iface := range ifaces {
		underlying := iface.Underlying().(*types.Interface)
		n := 0
		for _, c := range concrete {
			if types.Implements(c, underlying) {
				n++
			}
		}
		if n > counts[iface.Obj().Name()] {
			counts[iface.Obj().Name()] = n
		}
	}
	return counts
}

func loadMissed(path string) (map[rowKey]bool, []rowKey) {
	data, err := os.ReadFile(path)
	must(err)
	set := map[rowKey]bool{}
	var order []rowKey
	for _, line := range strings.Split(string(data), "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		cols := strings.Split(line, "\t")
		if len(cols) < 4 {
			continue
		}
		key := rowKey{cols[0], cols[1], cols[2], cols[3]}
		if !set[key] {
			set[key] = true
			order = append(order, key)
		}
	}
	return set, order
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
