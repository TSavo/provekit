// Leaf assertion translator.
//
// Recognizes a tight whitelist of testify / require call shapes and
// translates each side to an IrTerm. Anything else returns an error
// and the surrounding pattern handler decides whether to skip or warn.
//
// Whitelist (v0):
//
//	assert.Equal(t, <lhs>, <rhs>)        -> = (lhs, rhs)
//	require.Equal(t, <lhs>, <rhs>)       -> = (lhs, rhs)
//	assert.NotEqual(t, <lhs>, <rhs>)     -> ≠ (lhs, rhs)
//	require.NotEqual(t, <lhs>, <rhs>)    -> ≠ (lhs, rhs)
//	assert.True(t, <expr>)               -> true(expr)   (treated as bool atom)
//	assert.False(t, <expr>)              -> false(expr)
//	require.True(t, <expr>)              -> true(expr)
//	require.False(t, <expr>)             -> false(expr)
//	require.NoError(t, <err>)            -> = (err, nil)
//
// Each operand must be one of:
//   - identifier (Var)
//   - integer literal (Num)
//   - string literal (StrConst)
//   - simple binary comparison `lhs <op> rhs` where op is ==, !=, <,
//     <=, >, >= and both sides are operands themselves (Pattern-1
//     `assert.True(t, x >= 0)` form).
//   - single-arg call `Foo(arg)` (treated as a Ctor over the lifted arg).
//
// Anything else -> error.
//
// The error string is included verbatim in the structured warning and
// is intentionally short; report consumers grep these.

package lifgotests

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"strconv"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// LiftLeafAssertions is the Layer-0 entry point: it harvests each single
// top-level recognized assertion in a test file into its own `contract`
// ContractDeclaration with `inv = <assertion-formula>`. This is the
// single-assertion path the Layer-2 patterns RELEASE to (their warnings say
// "releasing to layer 0"); it lifts shapes like
//
//	assert.Equal(t, Double(3), 6)  ->  contract{ inv = =(Double(3), 6) }
//
// where `Double(3)` is a `ctor` named `Double` — exactly the harvested
// `=(<call>, <expected>)` callsite the verifier's body-discharge seam
// enumerates and reduces through the body-derived function-contract for
// `Double`. One contract per test function (Inv is the conjunction of that
// test's recognized assertions; for the common single-assertion case this is
// just the bare `=( ... )`), so a function-contract bridge can match it.
//
// Unlike LiftFile (Layer 2: bounded-loop / helper-inlining / multi-assert
// characterization), this does NOT require >= 2 assertions and does NOT fold
// into an opaque conjunction when there is exactly one — preserving the
// clean `=(<call>, <expected>)` shape the body-discharge enumerator needs.
func LiftLeafAssertions(src []byte, sourcePath string) ([]ir.ContractDeclaration, []LiftWarning, error) {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, sourcePath, src, 0)
	if err != nil {
		return nil, nil, fmt.Errorf("parse %s: %w", sourcePath, err)
	}
	var decls []ir.ContractDeclaration
	var warnings []LiftWarning
	for _, decl := range f.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || !isTestFunc(fn) || fn.Body == nil {
			continue
		}
		var atoms []ir.IrFormula
		for _, stmt := range fn.Body.List {
			call, ok := exprStmtCall(stmt)
			if !ok || !isAssertionCall(call) {
				continue
			}
			formula, err := liftAssertionCall(call)
			if err != nil {
				warnings = append(warnings, LiftWarning{
					Adapter: ADAPTER, SourcePath: sourcePath, ItemName: fn.Name.Name, Reason: err.Error(),
				})
				continue
			}
			atoms = append(atoms, formula)
		}
		if len(atoms) == 0 {
			continue
		}
		inv := atoms[0]
		if len(atoms) > 1 {
			inv = ir.And(atoms...)
		}
		decls = append(decls, ir.ContractDeclaration{
			Name:       fn.Name.Name,
			OutBinding: ir.DefaultOutBinding,
			Inv:        inv,
		})
	}
	return decls, warnings, nil
}

// isAssertionCall returns true if call is a recognized assertion-call
// shape. Used by Pattern 2 / 3 to filter top-level statements before
// committing to a pattern.
func isAssertionCall(call *ast.CallExpr) bool {
	pkg, name, ok := selectorParts(call.Fun)
	if !ok {
		return false
	}
	switch pkg + "." + name {
	case "assert.Equal", "require.Equal",
		"assert.NotEqual", "require.NotEqual",
		"assert.True", "require.True",
		"assert.False", "require.False",
		"require.NoError", "assert.NoError":
		return true
	}
	return false
}

// liftAssertionCall translates a recognized assertion call to a Formula.
// Returns an error if the call is not in the whitelist or any operand
// is not in the operand whitelist.
func liftAssertionCall(call *ast.CallExpr) (ir.IrFormula, error) {
	pkg, name, ok := selectorParts(call.Fun)
	if !ok {
		return nil, fmt.Errorf("not a selector call")
	}
	full := pkg + "." + name

	args := call.Args
	// Convention: testify takes t as the first arg. We strip it before
	// translating operand positions.
	if len(args) < 1 {
		return nil, fmt.Errorf("%s: zero arguments", full)
	}
	rest := args[1:]

	switch full {
	case "assert.Equal", "require.Equal":
		if len(rest) < 2 {
			return nil, fmt.Errorf("%s: expected 2 operand args, got %d", full, len(rest))
		}
		l, err := translateTerm(rest[0])
		if err != nil {
			return nil, fmt.Errorf("%s lhs: %w", full, err)
		}
		r, err := translateTerm(rest[1])
		if err != nil {
			return nil, fmt.Errorf("%s rhs: %w", full, err)
		}
		return ir.Eq(l, r), nil

	case "assert.NotEqual", "require.NotEqual":
		if len(rest) < 2 {
			return nil, fmt.Errorf("%s: expected 2 operand args, got %d", full, len(rest))
		}
		l, err := translateTerm(rest[0])
		if err != nil {
			return nil, fmt.Errorf("%s lhs: %w", full, err)
		}
		r, err := translateTerm(rest[1])
		if err != nil {
			return nil, fmt.Errorf("%s rhs: %w", full, err)
		}
		return ir.Neq(l, r), nil

	case "require.NoError", "assert.NoError":
		if len(rest) < 1 {
			return nil, fmt.Errorf("%s: expected an error operand", full)
		}
		l, err := translateTerm(rest[0])
		if err != nil {
			return nil, fmt.Errorf("%s err: %w", full, err)
		}
		// Encode as `err = nil` to match the proposition surface.
		return ir.Eq(l, ir.MakeVar("nil", ir.Ref)), nil

	case "assert.True", "require.True":
		if len(rest) < 1 {
			return nil, fmt.Errorf("%s: expected an expression", full)
		}
		// A boolean expression that's a comparison binop lifts to the
		// matching atomic predicate. Otherwise we treat the expression
		// as a single Term and emit `IsTrue(term)`.
		if be, ok := rest[0].(*ast.BinaryExpr); ok {
			f, err := translateBoolBinaryExpr(be)
			if err == nil {
				return f, nil
			}
		}
		t, err := translateTerm(rest[0])
		if err != nil {
			return nil, fmt.Errorf("%s expr: %w", full, err)
		}
		return ir.IsTrue(t), nil

	case "assert.False", "require.False":
		if len(rest) < 1 {
			return nil, fmt.Errorf("%s: expected an expression", full)
		}
		if be, ok := rest[0].(*ast.BinaryExpr); ok {
			f, err := translateBoolBinaryExpr(be)
			if err == nil {
				return ir.Not(f), nil
			}
		}
		t, err := translateTerm(rest[0])
		if err != nil {
			return nil, fmt.Errorf("%s expr: %w", full, err)
		}
		return ir.IsFalse(t), nil
	}

	return nil, fmt.Errorf("%s: not in v0 assertion whitelist", full)
}

// translateBoolBinaryExpr maps a Go binary comparison to the matching
// IR atomic predicate.
func translateBoolBinaryExpr(be *ast.BinaryExpr) (ir.IrFormula, error) {
	l, err := translateTerm(be.X)
	if err != nil {
		return nil, fmt.Errorf("binop lhs: %w", err)
	}
	r, err := translateTerm(be.Y)
	if err != nil {
		return nil, fmt.Errorf("binop rhs: %w", err)
	}
	switch be.Op {
	case token.EQL:
		return ir.Eq(l, r), nil
	case token.NEQ:
		return ir.Neq(l, r), nil
	case token.LSS:
		return ir.Lt(l, r), nil
	case token.LEQ:
		return ir.Lte(l, r), nil
	case token.GTR:
		return ir.Gt(l, r), nil
	case token.GEQ:
		return ir.Gte(l, r), nil
	default:
		return nil, fmt.Errorf("binop %s not supported", be.Op)
	}
}

// translateTerm translates a single operand expression to an IrTerm.
// The whitelist: identifier (Var), int literal (Num), string literal
// (StrConst), or single-arg call f(arg) (Ctor over the lifted arg).
func translateTerm(e ast.Expr) (ir.IrTerm, error) {
	switch x := e.(type) {
	case *ast.Ident:
		// `nil` and `true`/`false` keywords are distinguished from
		// generic identifiers only by name; the IR carries them as
		// named Vars (the verifier resolves keyword semantics later).
		return ir.MakeVar(x.Name, ir.Int), nil
	case *ast.BasicLit:
		switch x.Kind {
		case token.INT:
			n, err := strconv.ParseInt(x.Value, 0, 64)
			if err != nil {
				return nil, fmt.Errorf("int literal %q: %w", x.Value, err)
			}
			return ir.Num(n), nil
		case token.STRING:
			s, err := strconv.Unquote(x.Value)
			if err != nil {
				return nil, fmt.Errorf("string literal %q: %w", x.Value, err)
			}
			return ir.StrConst(s), nil
		default:
			return nil, fmt.Errorf("literal kind %s not supported", x.Kind)
		}
	case *ast.UnaryExpr:
		if x.Op == token.SUB {
			if lit, ok := x.X.(*ast.BasicLit); ok && lit.Kind == token.INT {
				n, err := strconv.ParseInt(lit.Value, 0, 64)
				if err != nil {
					return nil, fmt.Errorf("neg int literal: %w", err)
				}
				return ir.Num(-n), nil
			}
		}
		return nil, fmt.Errorf("unary %s not supported", x.Op)
	case *ast.CallExpr:
		// Single-arg call f(arg) -> ctor("f", [<arg>]). Reject
		// selector callees; only bare identifiers are ctor-shaped.
		id, ok := x.Fun.(*ast.Ident)
		if !ok {
			return nil, fmt.Errorf("call callee is not a bare identifier")
		}
		if len(x.Args) != 1 {
			return nil, fmt.Errorf("ctor %s: expected 1 arg, got %d", id.Name, len(x.Args))
		}
		inner, err := translateTerm(x.Args[0])
		if err != nil {
			return nil, fmt.Errorf("ctor %s arg: %w", id.Name, err)
		}
		return ctorTerm(id.Name, inner), nil
	case *ast.ParenExpr:
		return translateTerm(x.X)
	default:
		return nil, fmt.Errorf("operand %T not supported", e)
	}
}

// ctorTerm builds a generic 1-arg ctor term carrying the host call's
// callee name. Mirrors the Rust translator's translate_term path that
// emits {kind:"ctor", name, args} for `f(x)` shapes. We do NOT register
// the name with the extension / bridge registry: lift adapters do not
// own the verifier-resolved semantics of host-language calls; the
// Layer-3+ verifier (or an explicit user-authored bridge) does.
func ctorTerm(name string, arg ir.IrTerm) ir.IrTerm {
	return ir.MakeCtor(name, []ir.IrTerm{arg}, ir.Int)
}

// selectorParts returns the (pkg, name) parts of a `pkg.Name`
// selector-style call. Only top-level identifiers are accepted (no
// chained selectors like `a.b.c`).
func selectorParts(fn ast.Expr) (pkg, name string, ok bool) {
	sel, ok := fn.(*ast.SelectorExpr)
	if !ok {
		return "", "", false
	}
	x, ok := sel.X.(*ast.Ident)
	if !ok {
		return "", "", false
	}
	return x.Name, sel.Sel.Name, true
}
