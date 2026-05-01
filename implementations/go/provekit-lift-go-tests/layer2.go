// Layer 2 structural lift over `*_test.go` Go source.
//
// Patterns (port of provekit-lift-rust-tests/src/layer2.rs):
//
//	Pattern 1 — bounded for-loop as universal quantifier
//	Pattern 2 — helper-function inlining (one memento per call site)
//	Pattern 3 — multi-assertion characterization conjunction
//
// At most one pattern claims a given test function. The dispatcher
// uses Layer2Output.ClaimedTests so Layer 0 will skip those tests.
// When Pattern 3's filter selects < 2 liftable atoms, the claim is
// RELEASED (test name removed from the set) so Layer 0 can fall back
// to single-assertion atomic minting.

package lifgotests

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"strconv"
	"strings"

	"github.com/provekit/ir-symbolic/ir"
)

// LiftFile parses src as a Go file at sourcePath and runs the three-
// pattern Layer 2 lift over every `func TestX(t *testing.T)` it finds.
// The src bytes can be any well-formed Go source; non-test files are
// scanned too (helpers may live in any file).
func LiftFile(src []byte, sourcePath string) (*Layer2Output, error) {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, sourcePath, src, 0)
	if err != nil {
		return nil, fmt.Errorf("parse %s: %w", sourcePath, err)
	}
	return LiftAst(f, sourcePath), nil
}

// LiftAst is the AST-level entry point. Used by the unit tests so they
// don't have to round-trip through bytes.
func LiftAst(f *ast.File, sourcePath string) *Layer2Output {
	out := newLayer2Output()
	helpers := collectHelpers(f)
	for _, decl := range f.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		if !isTestFunc(fn) {
			continue
		}
		classifyAndLift(fn, sourcePath, helpers, out)
	}
	return out
}

// isTestFunc returns true for `func TestX(t *testing.T) { ... }` shape.
// Methods (receiver != nil) are excluded.
func isTestFunc(fn *ast.FuncDecl) bool {
	if fn.Recv != nil {
		return false
	}
	if fn.Name == nil || !strings.HasPrefix(fn.Name.Name, "Test") {
		return false
	}
	// Must be at least one param; the typical first param is *testing.T.
	if fn.Type.Params == nil || len(fn.Type.Params.List) == 0 {
		return false
	}
	return true
}

// helperDef matches the Rust adapter's HelperDef: a single typed
// parameter and a body of exactly one liftable assertion call.
type helperDef struct {
	paramName string
	// assertion is the `*ast.CallExpr` for the helper's single body
	// statement. We re-translate at each call site after substituting
	// the formal parameter for the argument expression.
	assertion *ast.CallExpr
}

func collectHelpers(f *ast.File) map[string]helperDef {
	helpers := map[string]helperDef{}
	for _, decl := range f.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		if fn.Recv != nil {
			continue
		}
		if fn.Name == nil {
			continue
		}
		// Skip Test* functions; they're never helpers.
		if strings.HasPrefix(fn.Name.Name, "Test") {
			continue
		}
		if h, ok := helperDefFromFn(fn); ok {
			helpers[fn.Name.Name] = h
		}
	}
	return helpers
}

func helperDefFromFn(fn *ast.FuncDecl) (helperDef, bool) {
	if fn.Type.Params == nil || len(fn.Type.Params.List) != 1 {
		return helperDef{}, false
	}
	field := fn.Type.Params.List[0]
	if len(field.Names) != 1 {
		return helperDef{}, false
	}
	pname := field.Names[0].Name
	if fn.Body == nil || len(fn.Body.List) != 1 {
		return helperDef{}, false
	}
	call, ok := exprStmtCall(fn.Body.List[0])
	if !ok {
		return helperDef{}, false
	}
	if !isAssertionCall(call) {
		return helperDef{}, false
	}
	return helperDef{paramName: pname, assertion: call}, true
}

// exprStmtCall returns the call expression of a statement that's an
// `ExpressionStatement(CallExpr)`. Otherwise returns false.
func exprStmtCall(s ast.Stmt) (*ast.CallExpr, bool) {
	es, ok := s.(*ast.ExprStmt)
	if !ok {
		return nil, false
	}
	c, ok := es.X.(*ast.CallExpr)
	if !ok {
		return nil, false
	}
	return c, true
}

// classifyAndLift selects which pattern owns `fn`. The first matching
// pattern wins; if none match, the test is left for Layer 0.
func classifyAndLift(fn *ast.FuncDecl, sourcePath string, helpers map[string]helperDef, out *Layer2Output) {
	testName := fn.Name.Name
	body := fn.Body
	if body == nil {
		return
	}
	stmts := body.List

	// PATTERN 1: single bounded for-loop with a single-stmt body.
	if len(stmts) == 1 {
		if forStmt, ok := stmts[0].(*ast.ForStmt); ok {
			classifyForLoop(forStmt, testName, sourcePath, out)
			return
		}
	}

	// PATTERN 2: every top-level stmt is a single-arg call to a known
	// helper, with at least one such call.
	calls := collectHelperCalls(stmts, helpers)
	if calls != nil && len(calls) > 0 {
		classifyHelperInlining(calls, helpers, testName, sourcePath, out)
		return
	}

	// PATTERN 3: every top-level stmt is a recognized assertion call
	// AND there are >= 2 of them.
	allAssertions := true
	var assertions []*ast.CallExpr
	for _, s := range stmts {
		c, ok := exprStmtCall(s)
		if !ok || !isAssertionCall(c) {
			allAssertions = false
			break
		}
		assertions = append(assertions, c)
	}
	if allAssertions && len(assertions) >= 2 {
		classifyCharacterization(assertions, testName, sourcePath, out)
		return
	}
	// Not a Layer-2 pattern. Leave for Layer 0.
}

// ---------------------------------------------------------------------
// Pattern 1: bounded for-loop -> forall-implies
// ---------------------------------------------------------------------

func classifyForLoop(forStmt *ast.ForStmt, testName, sourcePath string, out *Layer2Output) {
	// Claim the test regardless of liftability so Layer 0 doesn't
	// retry on the same body shape.
	out.claim(testName)
	out.Seen++

	// Init must be `<var> := <lo>`. Cond must be `<var> <op> <hi>` for
	// `<` or `<=`. Post must be `<var>++`.
	if forStmt.Init == nil || forStmt.Cond == nil || forStmt.Post == nil {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, "layer2 bounded-loop: only `for i := lo; i </<= hi; i++` shape supported in v0")
		return
	}
	varName, loTerm, ok := parseForInit(forStmt.Init)
	if !ok {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, "layer2 bounded-loop: init clause is not `<var> := <lit>`")
		return
	}
	hiTerm, inclusive, ok := parseForCond(forStmt.Cond, varName)
	if !ok {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, "layer2 bounded-loop: cond is not `<var> < <lit>` or `<var> <= <lit>`")
		return
	}
	if !parseForPostIncDec(forStmt.Post, varName) {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, "layer2 bounded-loop: post is not `<var>++`")
		return
	}

	// Body must be a single statement, no nested for-loops.
	bodyStmts := forStmt.Body.List
	if len(bodyStmts) != 1 {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, fmt.Sprintf(
			"layer2 bounded-loop: body has %d stmts (only single-stmt bodies in v0)", len(bodyStmts)))
		return
	}
	if hasNestedForLoop(bodyStmts[0]) {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName,
			"layer2 bounded-loop: nested for-loop detected; deferred to Layer 2.5")
		return
	}

	// Body stmt must be an assertion call.
	innerCall, ok := exprStmtCall(bodyStmts[0])
	if !ok || !isAssertionCall(innerCall) {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, "layer2 bounded-loop: body is not a recognized assertion call")
		return
	}
	innerFormula, err := liftAssertionCall(innerCall)
	if err != nil {
		out.BoundedLoopSkipped++
		out.warn(sourcePath, testName, fmt.Sprintf("layer2 bounded-loop: inner assertion not liftable: %v", err))
		return
	}

	// Build forall i:Int. (i >= lo AND i </<= hi) -> inner.
	// Order of operands and the choice of >= vs <= must match the Rust
	// reference adapter EXACTLY: the Rust impl uses `gte(var, lo)` for
	// the lower bound, NOT `lte(lo, var)`. Logically equivalent,
	// canonicalizes to different bytes (different predicate name +
	// different arg order). Cross-kit byte equivalence depends on this.
	quant := ir.ForAllNamed(varName, ir.Int, func(x ir.IrTerm) ir.IrFormula {
		lower := ir.Gte(x, loTerm)
		var upper ir.IrFormula
		if inclusive {
			upper = ir.Lte(x, hiTerm)
		} else {
			upper = ir.Lt(x, hiTerm)
		}
		ant := ir.And(lower, upper)
		return ir.Implies(ant, innerFormula)
	})

	out.Decls = append(out.Decls, ir.ContractDeclaration{
		Name:       testName,
		OutBinding: ir.DefaultOutBinding,
		Inv:        quant,
	})
	out.Lifted++
	out.BoundedLoopLifted++
}

// parseForInit parses `<ident> := <int-literal-or-neg>` and returns
// (varName, loTerm, ok).
func parseForInit(init ast.Stmt) (string, ir.IrTerm, bool) {
	asg, ok := init.(*ast.AssignStmt)
	if !ok || asg.Tok != token.DEFINE {
		return "", nil, false
	}
	if len(asg.Lhs) != 1 || len(asg.Rhs) != 1 {
		return "", nil, false
	}
	id, ok := asg.Lhs[0].(*ast.Ident)
	if !ok {
		return "", nil, false
	}
	loTerm, ok := literalIntOrIdent(asg.Rhs[0])
	if !ok {
		return "", nil, false
	}
	return id.Name, loTerm, true
}

// parseForCond parses `<varName> < <hi>` or `<varName> <= <hi>` and
// returns (hiTerm, inclusive, ok).
func parseForCond(cond ast.Expr, varName string) (ir.IrTerm, bool, bool) {
	be, ok := cond.(*ast.BinaryExpr)
	if !ok {
		return nil, false, false
	}
	if be.Op != token.LSS && be.Op != token.LEQ {
		return nil, false, false
	}
	id, ok := be.X.(*ast.Ident)
	if !ok || id.Name != varName {
		return nil, false, false
	}
	hi, ok := literalIntOrIdent(be.Y)
	if !ok {
		return nil, false, false
	}
	return hi, be.Op == token.LEQ, true
}

func parseForPostIncDec(post ast.Stmt, varName string) bool {
	inc, ok := post.(*ast.IncDecStmt)
	if !ok || inc.Tok != token.INC {
		return false
	}
	id, ok := inc.X.(*ast.Ident)
	return ok && id.Name == varName
}

// literalIntOrIdent accepts an int literal (possibly negated) or a
// bare identifier (treated as a free Int Var, mirrors Rust's
// literal_int_or_var).
func literalIntOrIdent(e ast.Expr) (ir.IrTerm, bool) {
	switch x := e.(type) {
	case *ast.BasicLit:
		if x.Kind != token.INT {
			return nil, false
		}
		n, err := strconv.ParseInt(x.Value, 0, 64)
		if err != nil {
			return nil, false
		}
		return ir.Num(n), true
	case *ast.UnaryExpr:
		if x.Op != token.SUB {
			return nil, false
		}
		lit, ok := x.X.(*ast.BasicLit)
		if !ok || lit.Kind != token.INT {
			return nil, false
		}
		n, err := strconv.ParseInt(lit.Value, 0, 64)
		if err != nil {
			return nil, false
		}
		return ir.Num(-n), true
	case *ast.Ident:
		return ir.MakeVar(x.Name, ir.Int), true
	case *ast.ParenExpr:
		return literalIntOrIdent(x.X)
	}
	return nil, false
}

// hasNestedForLoop returns true if stmt or anything nested inside it is
// a *ast.ForStmt or *ast.RangeStmt. We use ast.Inspect for the walk.
func hasNestedForLoop(stmt ast.Stmt) bool {
	found := false
	ast.Inspect(stmt, func(n ast.Node) bool {
		if found {
			return false
		}
		switch n.(type) {
		case *ast.ForStmt, *ast.RangeStmt:
			found = true
			return false
		}
		return true
	})
	return found
}

// ---------------------------------------------------------------------
// Pattern 2: helper inlining
// ---------------------------------------------------------------------

type helperCall struct {
	helperName string
	arg        ast.Expr
}

// collectHelperCalls returns the list of helper calls iff every
// top-level stmt is a single-arg call to a known helper. If any stmt
// fails the shape, returns nil (the caller falls through to Pattern 3).
func collectHelperCalls(stmts []ast.Stmt, helpers map[string]helperDef) []helperCall {
	var calls []helperCall
	for _, s := range stmts {
		c, ok := exprStmtCall(s)
		if !ok {
			return nil
		}
		id, ok := c.Fun.(*ast.Ident)
		if !ok {
			return nil
		}
		if _, known := helpers[id.Name]; !known {
			return nil
		}
		if len(c.Args) != 1 {
			return nil
		}
		calls = append(calls, helperCall{helperName: id.Name, arg: c.Args[0]})
	}
	return calls
}

func classifyHelperInlining(calls []helperCall, helpers map[string]helperDef, testName, sourcePath string, out *Layer2Output) {
	out.claim(testName)
	for i, c := range calls {
		out.Seen++
		mementoName := fmt.Sprintf("%s::call::%d", testName, i)
		helper := helpers[c.helperName]

		argTerm, err := translateTerm(c.arg)
		if err != nil {
			out.HelperInlinedSkipped++
			out.warn(sourcePath, mementoName, fmt.Sprintf("layer2 helper-inline: argument not liftable: %v", err))
			continue
		}
		raw, err := liftAssertionCall(helper.assertion)
		if err != nil {
			out.HelperInlinedSkipped++
			out.warn(sourcePath, mementoName, fmt.Sprintf("layer2 helper-inline: helper `%s` body not liftable: %v", c.helperName, err))
			continue
		}
		inlined := substVarInFormula(raw, helper.paramName, argTerm)
		out.Decls = append(out.Decls, ir.ContractDeclaration{
			Name:       mementoName,
			OutBinding: ir.DefaultOutBinding,
			Inv:        inlined,
		})
		out.Lifted++
		out.HelperInlinedLifted++
	}
}

// ---------------------------------------------------------------------
// Pattern 3: characterization conjunction
// ---------------------------------------------------------------------

func classifyCharacterization(assertions []*ast.CallExpr, testName, sourcePath string, out *Layer2Output) {
	out.claim(testName)
	out.Seen++

	var atoms []ir.IrFormula
	var skipped []string
	for i, a := range assertions {
		f, err := liftAssertionCall(a)
		if err != nil {
			skipped = append(skipped, fmt.Sprintf("#%d: %v", i, err))
			continue
		}
		atoms = append(atoms, f)
	}
	if len(atoms) < 2 {
		// Release the claim so Layer 0 can fall back to single-assert mint.
		out.unclaim(testName)
		out.CharacterizationSkipped++
		out.warn(sourcePath, testName, fmt.Sprintf(
			"layer2 characterization: only %d of %d asserts were liftable; releasing to layer 0",
			len(atoms), len(assertions)))
		return
	}

	out.Decls = append(out.Decls, ir.ContractDeclaration{
		Name:       testName,
		OutBinding: ir.DefaultOutBinding,
		Inv:        ir.And(atoms...),
	})
	out.Lifted++
	out.CharacterizationLifted++

	if len(skipped) > 0 {
		out.warn(sourcePath, testName, fmt.Sprintf(
			"layer2 characterization: %d atoms skipped from conjunction: %s",
			len(skipped), strings.Join(skipped, "; ")))
	}
}
