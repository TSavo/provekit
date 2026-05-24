package liftgo

import (
	"bytes"
	"encoding/hex"
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/printer"
	"go/token"
	"go/types"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/claim_envelope"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

type lifter struct {
	fset       *token.FileSet
	file       *ast.File
	pkg        *types.Package
	info       *types.Info
	path       string
	fnName     string
	locals     map[types.Object]bool
	knownFuncs map[string]bool
	effects    *effectSet
	// normalizeCoreArith selects the VERIFY-FACING op dialect: when true, the
	// arithmetic / comparison operators that map onto SMT-LIB core theories
	// (Int / Bool) are emitted with their core symbol (`*`, `+`, `<`, ...)
	// instead of the namespaced round-trip form (`go:mul`, `go:add`, ...).
	//
	// This mirrors Java's two-lifter split: JavaSourceLifter emits `java:mul`
	// (the round-trippable source dialect), ProductionWalk emits `*` (the
	// verify-facing dialect the z3-backed verifier discharges). The default
	// (false) keeps the byte-identical round-trip lift the existing tests and
	// the Go source compiler depend on; the verify-facing entry points
	// (LiftSourceCore / LiftPathsCore) set it true so the body-derived
	// `post = result == (* x 2)` is something z3 can actually reduce. The
	// verifier spine is NOT touched; Go is wired INTO it by speaking the op
	// vocabulary the spine already understands.
	normalizeCoreArith bool
}

// coreArithOp maps a namespaced Go op name to the SMT-LIB core-theory symbol
// the z3-backed verifier discharges, when one exists. Only the operators whose
// semantics are exactly an Int/Bool core operation are mapped; everything else
// (bitwise ops, shifts, dereference, ...) keeps its namespaced form because it
// has no faithful core-theory counterpart and must stay an uninterpreted /
// loudly-bounded symbol rather than silently aliasing to the wrong theory.
func coreArithOp(name string) (string, bool) {
	switch name {
	case "go:add":
		return "+", true
	case "go:sub":
		return "-", true
	case "go:mul":
		return "*", true
	case "go:div":
		return "div", true
	case "go:mod":
		return "mod", true
	case "go:eq":
		return "=", true
	case "go:lt":
		return "<", true
	case "go:le":
		return "<=", true
	case "go:gt":
		return ">", true
	case "go:ge":
		return ">=", true
	case "go:and":
		return "and", true
	case "go:or":
		return "or", true
	case "go:neg":
		return "-", true
	case "go:not":
		return "not", true
	default:
		return "", false
	}
}

// opForDialect returns the op name to emit for opName under this lifter's
// dialect. In the verify-facing dialect a known core-arith op is normalized to
// its SMT-LIB symbol; otherwise the namespaced name passes through unchanged.
func (l *lifter) opForDialect(opName string) string {
	if l.normalizeCoreArith {
		if core, ok := coreArithOp(opName); ok {
			return core
		}
	}
	return opName
}

type exprResult struct {
	term ir.IrTerm
	alg  any
	sort ir.Sort
}

type stmtResult struct {
	term      any
	ret       ir.IrTerm
	hasReturn bool
}

// LiftOptions selects the op dialect a lift emits and which functions it
// emits contracts for.
type LiftOptions struct {
	// NormalizeCoreArith emits the SMT-LIB core-theory symbol (`*`, `+`,
	// `<`, ...) for arithmetic / comparison operators instead of the
	// namespaced round-trip form (`go:mul`, ...). Set by the verify-facing
	// entry points so the body-derived postcondition is z3-dischargeable.
	NormalizeCoreArith bool
	// AnnotatedOnly gates contract emission on the AUTHORING declaration:
	// when true, only functions carrying a `//provekit:boundary(...)` or
	// `//provekit:sugar(...)` doc-comment directive are lifted. The
	// authoring surface (`go-bind` / `go-contracts` plugins) sets this so
	// the DECLARATION drives emission, mirroring rust's
	// `#[provekit::sugar(...)]` / `#[provekit::boundary(...)]`. The default
	// (false) keeps the emit-all behavior the bare `go` verify surface and
	// the round-trip lift depend on.
	AnnotatedOnly bool
}

// LiftSource lifts a single Go source file in the round-trip dialect
// (namespaced ops, byte-identical to the source compiler's expectation).
func LiftSource(packagePath, sourcePath string, source []byte) (LiftResult, error) {
	return LiftSourceWithOptions(packagePath, sourcePath, source, LiftOptions{})
}

// LiftSourceCore lifts a single Go source file in the VERIFY-FACING dialect:
// arithmetic / comparison ops are normalized to their SMT-LIB core symbols so
// the emitted `function-contract`'s `post = result == <body-expr>` discharges
// through the z3-backed verifier. Mirrors Java's ProductionWalk.
func LiftSourceCore(packagePath, sourcePath string, source []byte) (LiftResult, error) {
	return LiftSourceWithOptions(packagePath, sourcePath, source, LiftOptions{NormalizeCoreArith: true})
}

// LiftSourceWithOptions lifts a single Go source file under the given dialect.
func LiftSourceWithOptions(packagePath, sourcePath string, source []byte, opts LiftOptions) (LiftResult, error) {
	if packagePath == "" {
		packagePath = "command-line-arguments"
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, source, parser.ParseComments)
	if err != nil {
		return LiftResult{}, err
	}

	info := &types.Info{
		Types:      map[ast.Expr]types.TypeAndValue{},
		Defs:       map[*ast.Ident]types.Object{},
		Uses:       map[*ast.Ident]types.Object{},
		Selections: map[*ast.SelectorExpr]*types.Selection{},
	}
	var diagnostics []Diagnostic
	conf := types.Config{
		Importer: importer.Default(),
		Error: func(err error) {
			diagnostics = append(diagnostics, Diagnostic{Path: sourcePath, Message: err.Error()})
		},
	}
	pkg, _ := conf.Check(packagePath, fset, []*ast.File{file}, info)
	if pkg == nil {
		pkg = types.NewPackage(packagePath, file.Name.Name)
	}

	known := map[string]bool{}
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		if obj, ok := info.Defs[fn.Name].(*types.Func); ok {
			known[obj.FullName()] = true
		} else {
			known[fallbackFuncName(packagePath, fn)] = true
		}
	}

	var result LiftResult
	result.Diagnostics = diagnostics
	result.Annotations = map[string]*Annotation{}
	var bodyTerms []any
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		// Authoring surface: the DECLARATION drives emission. A malformed
		// `//provekit:` directive is refused loudly (an author who typed it
		// meant to declare something), not silently dropped.
		ann, annErr := parseFuncAnnotation(fn)
		if annErr != nil {
			result.Refusals = append(result.Refusals, Refusal{
				Kind:     "malformed-annotation",
				Function: fallbackFuncName(packagePath, fn),
				Line:     fset.Position(fn.Pos()).Line,
				Reason:   annErr.Error(),
			})
			continue
		}
		if opts.AnnotatedOnly && ann == nil {
			// No boundary/sugar declared: the author did not ask the
			// authoring surface to lift this function.
			continue
		}
		contract, bodyTerm, refusals := liftFunc(fset, file, pkg, info, sourcePath, known, packagePath, fn, opts)
		if len(refusals) > 0 {
			result.Refusals = append(result.Refusals, refusals...)
			continue
		}
		result.Contracts = append(result.Contracts, contract)
		result.IR = append(result.IR, contract)
		bodyTerms = append(bodyTerms, bodyTerm)
		if ann != nil {
			result.Annotations[contract.FnName] = ann
		}
	}

	if len(bodyTerms) > 0 {
		body := foldSeq(bodyTerms)
		sourceTerm := op("go:source-unit", map[string]any{
			"kind":     "bytes",
			"encoding": "hex",
			"value":    hex.EncodeToString(source),
		}, body)
		sourceCID, _, err := canonicalCID(sourceTerm)
		if err != nil {
			return LiftResult{}, err
		}
		su := SourceUnit{
			Kind:          "go-source-unit",
			SchemaVersion: "1",
			Source:        sourcePath,
			SourceCid:     sourceCID,
			Signature:     Version,
			Term:          sourceTerm,
		}
		result.SourceUnits = append(result.SourceUnits, su)
		result.IR = append(result.IR, su)
	}
	return result, nil
}

func liftFunc(fset *token.FileSet, file *ast.File, pkg *types.Package, info *types.Info, sourcePath string, known map[string]bool, packagePath string, fn *ast.FuncDecl, opts LiftOptions) (FunctionContract, any, []Refusal) {
	fnName := fallbackFuncName(packagePath, fn)
	if obj, ok := info.Defs[fn.Name].(*types.Func); ok {
		fnName = obj.FullName()
	}
	if fn.Recv != nil && !receiverTypeResolved(info, fn) {
		pos := fset.Position(fn.Name.Pos())
		return FunctionContract{}, nil, []Refusal{{
			Kind: "unresolved-receiver-type", Function: unresolvedReceiverFunctionName(sourcePath, pos, fn), Line: pos.Line,
			Reason: "receiver type could not be resolved to a named type",
		}}
	}
	refuse := func(kind string, pos token.Pos, reason string) []Refusal {
		return []Refusal{{Kind: kind, Function: fnName, Line: fset.Position(pos).Line, Reason: reason}}
	}
	if fn.Type.TypeParams != nil && len(fn.Type.TypeParams.List) > 0 {
		return FunctionContract{}, nil, refuse("unsupported-generics", fn.Type.TypeParams.Pos(), "generic functions are not modeled by the Go source lifter")
	}
	if fn.Body == nil {
		return FunctionContract{}, nil, refuse("missing-body", fn.Pos(), "function declaration has no body")
	}
	formals, formalSorts, localObjects, err := extractFormals(info, fn)
	if err != nil {
		return FunctionContract{}, nil, refuse("unsupported-signature", fn.Pos(), err.Error())
	}
	returnSort, hasResult, err := extractReturnSort(info, fn)
	if err != nil {
		return FunctionContract{}, nil, refuse("unsupported-signature", fn.Pos(), err.Error())
	}

	l := &lifter{
		fset:               fset,
		file:               file,
		pkg:                pkg,
		info:               info,
		path:               sourcePath,
		fnName:             fnName,
		locals:             localObjects,
		knownFuncs:         known,
		effects:            newEffectSet(),
		normalizeCoreArith: opts.NormalizeCoreArith,
	}
	body, err := l.liftBlock(fn.Body.List)
	if err != nil {
		return FunctionContract{}, nil, refuse("unsupported-syntax", errPos(err, fn.Pos()), err.Error())
	}
	pre, err := formulaValue(ir.And())
	if err != nil {
		return FunctionContract{}, nil, refuse("internal-error", fn.Pos(), err.Error())
	}
	postFormula := ir.And()
	if body.hasReturn {
		postFormula = ir.Eq(ir.MakeVar("result", resultIRSort(fn, info)), body.ret)
	} else if _, panics := l.effects.byKey["4:panics"]; hasResult && !panics {
		return FunctionContract{}, nil, refuse("unsupported-control-flow", fn.Pos(), "non-void function has no modeled return")
	}
	post, err := formulaValue(postFormula)
	if err != nil {
		return FunctionContract{}, nil, refuse("internal-error", fn.Pos(), err.Error())
	}
	bodyCID, _, err := canonicalCID(body.term)
	if err != nil {
		return FunctionContract{}, nil, refuse("internal-error", fn.Pos(), err.Error())
	}
	fileName := sourcePath
	pos := fset.Position(fn.Name.Pos())
	contract := FunctionContract{
		AutoMintedMementos: []any{},
		BodyCid:            &bodyCID,
		Effects:            l.effects.sorted(),
		FnName:             fnName,
		FormalSorts:        formalSorts,
		Formals:            formals,
		Kind:               "function-contract",
		Locus:              Locus{File: &fileName, Line: pos.Line, Col: pos.Column},
		Post:               post,
		Pre:                pre,
		ReturnSort:         returnSort,
		SchemaVersion:      "1",
	}
	return contract, body.term, nil
}

func extractFormals(info *types.Info, fn *ast.FuncDecl) ([]string, []any, map[types.Object]bool, error) {
	locals := map[types.Object]bool{}
	var names []string
	var sorts []any
	if fn.Recv != nil {
		for _, field := range fn.Recv.List {
			if len(field.Names) != 1 {
				return nil, nil, nil, fmt.Errorf("receiver must have exactly one name")
			}
			name := field.Names[0].Name
			names = append(names, name)
			sorts = append(sorts, sortValue(typeOfExpr(info, field.Type)))
			if obj := info.Defs[field.Names[0]]; obj != nil {
				locals[obj] = true
			}
		}
	}
	if fn.Type.Params != nil {
		for _, field := range fn.Type.Params.List {
			if len(field.Names) == 0 {
				return nil, nil, nil, fmt.Errorf("unnamed parameters are refused to keep formals deterministic")
			}
			for _, name := range field.Names {
				names = append(names, name.Name)
				sorts = append(sorts, sortValue(typeOfExpr(info, field.Type)))
				if obj := info.Defs[name]; obj != nil {
					locals[obj] = true
				}
			}
		}
	}
	return names, sorts, locals, nil
}

func extractReturnSort(info *types.Info, fn *ast.FuncDecl) (any, bool, error) {
	if fn.Type.Results == nil || len(fn.Type.Results.List) == 0 {
		return primitiveSortValue("Unit"), false, nil
	}
	count := 0
	var typ types.Type
	for _, field := range fn.Type.Results.List {
		n := len(field.Names)
		if n == 0 {
			n = 1
		}
		count += n
		typ = typeOfExpr(info, field.Type)
	}
	if count != 1 {
		return nil, false, fmt.Errorf("only zero or one result is supported, got %d", count)
	}
	return sortValue(typ), true, nil
}

func resultIRSort(fn *ast.FuncDecl, info *types.Info) ir.Sort {
	if fn.Type.Results == nil || len(fn.Type.Results.List) == 0 {
		return ir.Ref
	}
	return irSort(typeOfExpr(info, fn.Type.Results.List[0].Type))
}

type liftError struct {
	pos token.Pos
	msg string
}

func (e liftError) Error() string { return e.msg }

func errAt(pos token.Pos, format string, args ...any) error {
	return liftError{pos: pos, msg: fmt.Sprintf(format, args...)}
}

func errPos(err error, fallback token.Pos) token.Pos {
	if e, ok := err.(liftError); ok {
		return e.pos
	}
	return fallback
}

func (l *lifter) liftBlock(stmts []ast.Stmt) (stmtResult, error) {
	if len(stmts) == 0 {
		return stmtResult{term: op("go:skip")}, nil
	}
	var terms []any
	var ret ir.IrTerm
	hasReturn := false
	for _, stmt := range stmts {
		lifted, err := l.liftStmt(stmt)
		if err != nil {
			return stmtResult{}, err
		}
		terms = append(terms, lifted.term)
		if lifted.hasReturn {
			ret = lifted.ret
			hasReturn = true
		}
	}
	return stmtResult{term: foldSeq(terms), ret: ret, hasReturn: hasReturn}, nil
}

func (l *lifter) liftStmt(stmt ast.Stmt) (stmtResult, error) {
	switch s := stmt.(type) {
	case *ast.ReturnStmt:
		if len(s.Results) > 1 {
			return stmtResult{}, errAt(s.Pos(), "return with %d values is not modeled", len(s.Results))
		}
		if len(s.Results) == 0 {
			return stmtResult{term: op("go:return"), hasReturn: true}, nil
		}
		expr, err := l.liftExpr(s.Results[0])
		if err != nil {
			return stmtResult{}, err
		}
		return stmtResult{term: op("go:return", expr.alg), ret: expr.term, hasReturn: true}, nil
	case *ast.AssignStmt:
		return l.liftAssign(s)
	case *ast.DeclStmt:
		return l.liftDeclStmt(s)
	case *ast.ExprStmt:
		expr, err := l.liftExpr(s.X)
		if err != nil {
			return stmtResult{}, err
		}
		return stmtResult{term: expr.alg}, nil
	case *ast.IfStmt:
		return l.liftIf(s)
	case *ast.ForStmt:
		return l.liftFor(s)
	case *ast.RangeStmt:
		return l.liftRange(s)
	case *ast.IncDecStmt:
		target, err := l.liftTarget(s.X)
		if err != nil {
			return stmtResult{}, err
		}
		l.addWriteEffectForTarget(s.X)
		return stmtResult{term: op("go:incdec", target, map[string]any{"kind": "literal", "value": s.Tok.String()})}, nil
	case *ast.EmptyStmt:
		return stmtResult{term: op("go:skip")}, nil
	case *ast.GoStmt:
		return stmtResult{}, errAt(s.Pos(), "go statements are not modeled")
	case *ast.DeferStmt:
		return stmtResult{}, errAt(s.Pos(), "defer statements are not modeled")
	case *ast.SendStmt:
		return stmtResult{}, errAt(s.Pos(), "channel send statements are not modeled")
	case *ast.SelectStmt:
		return stmtResult{}, errAt(s.Pos(), "select statements are not modeled")
	case *ast.SwitchStmt, *ast.TypeSwitchStmt:
		return stmtResult{}, errAt(s.Pos(), "switch statements are not modeled")
	case *ast.BranchStmt, *ast.LabeledStmt:
		return stmtResult{}, errAt(s.Pos(), "%T is not modeled", s)
	default:
		return stmtResult{}, errAt(stmt.Pos(), "%T is not modeled", stmt)
	}
}

func (l *lifter) liftAssign(s *ast.AssignStmt) (stmtResult, error) {
	if len(s.Lhs) != len(s.Rhs) {
		return stmtResult{}, errAt(s.Pos(), "assignment arity mismatch is not modeled")
	}
	var terms []any
	for i, lhs := range s.Lhs {
		target, err := l.liftTarget(lhs)
		if err != nil {
			return stmtResult{}, err
		}
		value, err := l.liftExpr(s.Rhs[i])
		if err != nil {
			return stmtResult{}, err
		}
		switch s.Tok {
		case token.DEFINE:
			if id, ok := lhs.(*ast.Ident); ok {
				if obj := l.info.Defs[id]; obj != nil {
					l.locals[obj] = true
				}
			}
			terms = append(terms, op("go:decl", target, value.alg))
		case token.ASSIGN:
			l.addWriteEffectForTarget(lhs)
			terms = append(terms, op("go:assign", target, value.alg))
		case token.ADD_ASSIGN, token.SUB_ASSIGN, token.MUL_ASSIGN, token.QUO_ASSIGN, token.REM_ASSIGN, token.AND_ASSIGN, token.OR_ASSIGN, token.XOR_ASSIGN, token.SHL_ASSIGN, token.SHR_ASSIGN:
			l.addWriteEffectForTarget(lhs)
			opName, ok := compoundOp(s.Tok)
			if !ok {
				return stmtResult{}, errAt(s.Pos(), "compound assignment %s is not modeled", s.Tok)
			}
			terms = append(terms, op("go:assign", target, op(opName, target, value.alg)))
		default:
			return stmtResult{}, errAt(s.Pos(), "assignment token %s is not modeled", s.Tok)
		}
	}
	return stmtResult{term: foldSeq(terms)}, nil
}

func (l *lifter) liftDeclStmt(s *ast.DeclStmt) (stmtResult, error) {
	gen, ok := s.Decl.(*ast.GenDecl)
	if !ok || gen.Tok != token.VAR {
		return stmtResult{}, errAt(s.Pos(), "only var declarations are modeled")
	}
	var terms []any
	for _, spec := range gen.Specs {
		valueSpec, ok := spec.(*ast.ValueSpec)
		if !ok {
			return stmtResult{}, errAt(spec.Pos(), "%T declaration is not modeled", spec)
		}
		for i, name := range valueSpec.Names {
			var value any = op("go:skip")
			if i < len(valueSpec.Values) {
				lifted, err := l.liftExpr(valueSpec.Values[i])
				if err != nil {
					return stmtResult{}, err
				}
				value = lifted.alg
			}
			if obj := l.info.Defs[name]; obj != nil {
				l.locals[obj] = true
			}
			terms = append(terms, op("go:decl", varAlg(name.Name), value))
		}
	}
	return stmtResult{term: foldSeq(terms)}, nil
}

func (l *lifter) liftIf(s *ast.IfStmt) (stmtResult, error) {
	var initTerm any = op("go:skip")
	if s.Init != nil {
		init, err := l.liftStmt(s.Init)
		if err != nil {
			return stmtResult{}, err
		}
		initTerm = init.term
	}
	cond, err := l.liftExpr(s.Cond)
	if err != nil {
		return stmtResult{}, err
	}
	thenBranch, err := l.liftBlock(s.Body.List)
	if err != nil {
		return stmtResult{}, err
	}
	elseBranch := stmtResult{term: op("go:skip")}
	if s.Else != nil {
		switch e := s.Else.(type) {
		case *ast.BlockStmt:
			elseBranch, err = l.liftBlock(e.List)
		case *ast.IfStmt:
			elseBranch, err = l.liftIf(e)
		default:
			err = errAt(e.Pos(), "else branch %T is not modeled", e)
		}
		if err != nil {
			return stmtResult{}, err
		}
	}
	term := op("go:if", initTerm, cond.alg, thenBranch.term, elseBranch.term)
	if thenBranch.hasReturn && elseBranch.hasReturn {
		ret := ir.MakeCtor("go:if", []ir.IrTerm{cond.term, thenBranch.ret, elseBranch.ret}, thenBranch.ret.TermSort())
		return stmtResult{term: term, ret: ret, hasReturn: true}, nil
	}
	return stmtResult{term: term}, nil
}

func (l *lifter) liftFor(s *ast.ForStmt) (stmtResult, error) {
	var initTerm any = op("go:skip")
	if s.Init != nil {
		init, err := l.liftStmt(s.Init)
		if err != nil {
			return stmtResult{}, err
		}
		initTerm = init.term
	}
	var condTerm any = op("go:skip")
	if s.Cond != nil {
		cond, err := l.liftExpr(s.Cond)
		if err != nil {
			return stmtResult{}, err
		}
		condTerm = cond.alg
	}
	var postTerm any = op("go:skip")
	if s.Post != nil {
		post, err := l.liftStmt(s.Post)
		if err != nil {
			return stmtResult{}, err
		}
		postTerm = post.term
	}
	body, err := l.liftBlock(s.Body.List)
	if err != nil {
		return stmtResult{}, err
	}
	term := op("go:for", initTerm, condTerm, postTerm, body.term)
	l.addOpaqueLoop(term)
	return stmtResult{term: term}, nil
}

func (l *lifter) liftRange(s *ast.RangeStmt) (stmtResult, error) {
	rangeExpr, err := l.liftExpr(s.X)
	if err != nil {
		return stmtResult{}, err
	}
	var key any = op("go:skip")
	if s.Key != nil {
		key, err = l.liftTarget(s.Key)
		if err != nil {
			return stmtResult{}, err
		}
	}
	var value any = op("go:skip")
	if s.Value != nil {
		value, err = l.liftTarget(s.Value)
		if err != nil {
			return stmtResult{}, err
		}
	}
	body, err := l.liftBlock(s.Body.List)
	if err != nil {
		return stmtResult{}, err
	}
	term := op("go:range", key, value, rangeExpr.alg, body.term)
	l.addOpaqueLoop(term)
	return stmtResult{term: term}, nil
}

func (l *lifter) liftExpr(expr ast.Expr) (exprResult, error) {
	switch e := expr.(type) {
	case *ast.BasicLit:
		return liftLiteral(e)
	case *ast.Ident:
		return l.liftIdent(e)
	case *ast.BinaryExpr:
		left, err := l.liftExpr(e.X)
		if err != nil {
			return exprResult{}, err
		}
		right, err := l.liftExpr(e.Y)
		if err != nil {
			return exprResult{}, err
		}
		opName, ok := binaryOp(e.Op)
		if !ok {
			return exprResult{}, errAt(e.OpPos, "binary operator %s is not modeled", e.Op)
		}
		opName = l.opForDialect(opName)
		sort := irSort(l.info.Types[e].Type)
		return exprResult{term: ir.MakeCtor(opName, []ir.IrTerm{left.term, right.term}, sort), alg: op(opName, left.alg, right.alg), sort: sort}, nil
	case *ast.UnaryExpr:
		inner, err := l.liftExpr(e.X)
		if err != nil {
			return exprResult{}, err
		}
		opName, ok := unaryOp(e.Op)
		if !ok {
			return exprResult{}, errAt(e.OpPos, "unary operator %s is not modeled", e.Op)
		}
		opName = l.opForDialect(opName)
		sort := irSort(l.info.Types[e].Type)
		return exprResult{term: ir.MakeCtor(opName, []ir.IrTerm{inner.term}, sort), alg: op(opName, inner.alg), sort: sort}, nil
	case *ast.StarExpr:
		inner, err := l.liftExpr(e.X)
		if err != nil {
			return exprResult{}, err
		}
		sort := irSort(l.info.Types[e].Type)
		return exprResult{term: ir.MakeCtor("go:deref", []ir.IrTerm{inner.term}, sort), alg: op("go:deref", inner.alg), sort: sort}, nil
	case *ast.ParenExpr:
		return l.liftExpr(e.X)
	case *ast.CallExpr:
		return l.liftCall(e)
	case *ast.SelectorExpr:
		base, err := l.liftExpr(e.X)
		if err != nil {
			return exprResult{}, err
		}
		sort := irSort(l.info.Types[e].Type)
		return exprResult{term: ir.MakeCtor("go:member", []ir.IrTerm{base.term, ir.StrConst(e.Sel.Name)}, sort), alg: op("go:member", base.alg, map[string]any{"kind": "identifier", "name": e.Sel.Name}), sort: sort}, nil
	case *ast.IndexExpr:
		base, err := l.liftExpr(e.X)
		if err != nil {
			return exprResult{}, err
		}
		index, err := l.liftExpr(e.Index)
		if err != nil {
			return exprResult{}, err
		}
		sort := irSort(l.info.Types[e].Type)
		return exprResult{term: ir.MakeCtor("go:index", []ir.IrTerm{base.term, index.term}, sort), alg: op("go:index", base.alg, index.alg), sort: sort}, nil
	case *ast.CompositeLit:
		return exprResult{}, errAt(e.Pos(), "composite literals are not modeled")
	case *ast.FuncLit:
		return exprResult{}, errAt(e.Pos(), "function literals are not modeled")
	case *ast.ChanType:
		return exprResult{}, errAt(e.Pos(), "channels are not modeled")
	default:
		return exprResult{}, errAt(expr.Pos(), "expression %T is not modeled", expr)
	}
}

func (l *lifter) liftIdent(id *ast.Ident) (exprResult, error) {
	switch id.Name {
	case "true":
		return exprResult{term: ir.BoolConst(true), alg: map[string]any{"kind": "literal", "value": true}, sort: ir.Bool}, nil
	case "false":
		return exprResult{term: ir.BoolConst(false), alg: map[string]any{"kind": "literal", "value": false}, sort: ir.Bool}, nil
	case "nil":
		return exprResult{term: ir.MakeVar("nil", ir.Ref), alg: varAlg("nil"), sort: ir.Ref}, nil
	}
	if obj := l.info.Uses[id]; obj != nil && l.isPackageVar(obj) {
		l.effects.add(Effect{Kind: "reads", Target: objectCell(obj)})
	}
	sort := irSort(l.info.Types[id].Type)
	return exprResult{term: ir.MakeVar(id.Name, sort), alg: varAlg(id.Name), sort: sort}, nil
}

func (l *lifter) liftCall(call *ast.CallExpr) (exprResult, error) {
	calleeName := l.calleeName(call.Fun)
	var args []ir.IrTerm
	var algArgs []any
	for _, arg := range call.Args {
		lifted, err := l.liftExpr(arg)
		if err != nil {
			return exprResult{}, err
		}
		args = append(args, lifted.term)
		algArgs = append(algArgs, lifted.alg)
	}
	if calleeName == "panic" {
		l.effects.add(Effect{Kind: "panics"})
	} else if isIOCall(calleeName) {
		l.effects.add(Effect{Kind: "io"})
	} else if calleeName == "unsafe" || strings.HasPrefix(calleeName, "unsafe.") {
		l.effects.add(Effect{Kind: "unsafe"})
	} else if calleeName != "" && !l.knownFuncs[calleeName] && !isPureBuiltin(calleeName) {
		l.effects.add(Effect{Kind: "unresolved_call", Name: calleeName})
	}
	calleeTerm := ir.StrConst(calleeName)
	termArgs := append([]ir.IrTerm{calleeTerm}, args...)
	alg := op("go:call", append([]any{map[string]any{"kind": "identifier", "name": calleeName}}, algArgs...)...)
	sort := irSort(l.info.Types[call].Type)
	return exprResult{term: ir.MakeCtor("go:call", termArgs, sort), alg: alg, sort: sort}, nil
}

func (l *lifter) liftTarget(expr ast.Expr) (any, error) {
	switch e := expr.(type) {
	case *ast.Ident:
		return varAlg(e.Name), nil
	case *ast.SelectorExpr:
		base, err := l.liftTarget(e.X)
		if err != nil {
			return nil, err
		}
		return op("go:member", base, map[string]any{"kind": "identifier", "name": e.Sel.Name}), nil
	case *ast.IndexExpr:
		base, err := l.liftExpr(e.X)
		if err != nil {
			return nil, err
		}
		index, err := l.liftExpr(e.Index)
		if err != nil {
			return nil, err
		}
		return op("go:index", base.alg, index.alg), nil
	case *ast.StarExpr:
		inner, err := l.liftExpr(e.X)
		if err != nil {
			return nil, err
		}
		return op("go:deref", inner.alg), nil
	case *ast.ParenExpr:
		return l.liftTarget(e.X)
	default:
		return nil, errAt(expr.Pos(), "assignment target %T is not modeled", expr)
	}
}

func (l *lifter) addWriteEffectForTarget(expr ast.Expr) {
	switch e := expr.(type) {
	case *ast.Ident:
		if obj := l.info.Uses[e]; obj != nil && l.isPackageVar(obj) {
			l.effects.add(Effect{Kind: "writes", Target: objectCell(obj)})
		}
	case *ast.StarExpr:
		l.effects.add(Effect{Kind: "writes", Target: "*" + exprString(l.fset, e.X)})
	case *ast.SelectorExpr:
		if !l.exprIsLocal(e.X) {
			l.effects.add(Effect{Kind: "writes", Target: exprString(l.fset, expr)})
		}
	case *ast.IndexExpr:
		if !l.exprIsLocal(e.X) {
			l.effects.add(Effect{Kind: "writes", Target: exprString(l.fset, expr)})
		}
	}
}

func (l *lifter) addOpaqueLoop(term any) {
	cid, _, err := canonicalCID(term)
	if err == nil {
		l.effects.add(Effect{Kind: "opaque_loop", LoopCid: cid})
	}
}

func (l *lifter) isPackageVar(obj types.Object) bool {
	v, ok := obj.(*types.Var)
	if !ok || v.Pkg() == nil {
		return false
	}
	if l.locals[obj] {
		return false
	}
	return obj.Parent() == l.pkg.Scope()
}

func (l *lifter) exprIsLocal(expr ast.Expr) bool {
	id, ok := expr.(*ast.Ident)
	if !ok {
		return false
	}
	obj := l.info.Uses[id]
	return obj != nil && l.locals[obj]
}

func (l *lifter) calleeName(fun ast.Expr) string {
	if obj := objectForCallee(l.info, fun); obj != nil {
		if fn, ok := obj.(*types.Func); ok {
			return fn.FullName()
		}
		if obj.Pkg() != nil {
			return obj.Pkg().Path() + "." + obj.Name()
		}
		return obj.Name()
	}
	switch f := fun.(type) {
	case *ast.Ident:
		return f.Name
	case *ast.SelectorExpr:
		return selectorName(f)
	default:
		return exprString(l.fset, fun)
	}
}

func objectForCallee(info *types.Info, fun ast.Expr) types.Object {
	switch f := fun.(type) {
	case *ast.Ident:
		return info.Uses[f]
	case *ast.SelectorExpr:
		if sel := info.Selections[f]; sel != nil {
			return sel.Obj()
		}
		return info.Uses[f.Sel]
	default:
		return nil
	}
}

func selectorName(sel *ast.SelectorExpr) string {
	parts := []string{sel.Sel.Name}
	for {
		x, ok := sel.X.(*ast.SelectorExpr)
		if !ok {
			break
		}
		parts = append([]string{x.Sel.Name}, parts...)
		sel = x
	}
	if id, ok := sel.X.(*ast.Ident); ok {
		parts = append([]string{id.Name}, parts...)
	}
	return strings.Join(parts, ".")
}

func liftLiteral(lit *ast.BasicLit) (exprResult, error) {
	switch lit.Kind {
	case token.INT:
		n, err := strconv.ParseInt(lit.Value, 0, 64)
		if err != nil {
			return exprResult{}, errAt(lit.Pos(), "int literal %q: %v", lit.Value, err)
		}
		return exprResult{term: ir.Num(n), alg: map[string]any{"kind": "literal", "value": n}, sort: ir.Int}, nil
	case token.FLOAT:
		f, err := strconv.ParseFloat(lit.Value, 64)
		if err != nil {
			return exprResult{}, errAt(lit.Pos(), "float literal %q: %v", lit.Value, err)
		}
		return exprResult{term: ir.RealConst(f), alg: map[string]any{"kind": "literal", "value": f}, sort: ir.Real}, nil
	case token.STRING:
		s, err := strconv.Unquote(lit.Value)
		if err != nil {
			return exprResult{}, errAt(lit.Pos(), "string literal %q: %v", lit.Value, err)
		}
		return exprResult{term: ir.StrConst(s), alg: map[string]any{"kind": "literal", "value": s}, sort: ir.String}, nil
	case token.CHAR:
		s, err := strconv.Unquote(lit.Value)
		if err != nil || len([]rune(s)) != 1 {
			return exprResult{}, errAt(lit.Pos(), "char literal %q is not modeled", lit.Value)
		}
		return exprResult{term: ir.Num(int64([]rune(s)[0])), alg: map[string]any{"kind": "literal", "value": int64([]rune(s)[0])}, sort: ir.Int}, nil
	default:
		return exprResult{}, errAt(lit.Pos(), "literal kind %s is not modeled", lit.Kind)
	}
}

func binaryOp(tok token.Token) (string, bool) {
	switch tok {
	case token.ADD:
		return "go:add", true
	case token.SUB:
		return "go:sub", true
	case token.MUL:
		return "go:mul", true
	case token.QUO:
		return "go:div", true
	case token.REM:
		return "go:mod", true
	case token.EQL:
		return "go:eq", true
	case token.NEQ:
		return "go:ne", true
	case token.LSS:
		return "go:lt", true
	case token.LEQ:
		return "go:le", true
	case token.GTR:
		return "go:gt", true
	case token.GEQ:
		return "go:ge", true
	case token.LAND:
		return "go:and", true
	case token.LOR:
		return "go:or", true
	case token.AND:
		return "go:bitand", true
	case token.OR:
		return "go:bitor", true
	case token.XOR:
		return "go:bitxor", true
	case token.SHL:
		return "go:shl", true
	case token.SHR:
		return "go:shr", true
	default:
		return "", false
	}
}

func compoundOp(tok token.Token) (string, bool) {
	switch tok {
	case token.ADD_ASSIGN:
		return "go:add", true
	case token.SUB_ASSIGN:
		return "go:sub", true
	case token.MUL_ASSIGN:
		return "go:mul", true
	case token.QUO_ASSIGN:
		return "go:div", true
	case token.REM_ASSIGN:
		return "go:mod", true
	case token.AND_ASSIGN:
		return "go:bitand", true
	case token.OR_ASSIGN:
		return "go:bitor", true
	case token.XOR_ASSIGN:
		return "go:bitxor", true
	case token.SHL_ASSIGN:
		return "go:shl", true
	case token.SHR_ASSIGN:
		return "go:shr", true
	default:
		return "", false
	}
}

func unaryOp(tok token.Token) (string, bool) {
	switch tok {
	case token.NOT:
		return "go:not", true
	case token.XOR:
		return "go:bitnot", true
	case token.SUB:
		return "go:neg", true
	case token.AND:
		return "go:addr", true
	default:
		return "", false
	}
}

func op(name string, args ...any) map[string]any {
	return map[string]any{"kind": "op", "name": name, "args": args}
}

func varAlg(name string) map[string]any {
	return map[string]any{"kind": "var", "name": name}
}

func foldSeq(terms []any) any {
	if len(terms) == 0 {
		return op("go:skip")
	}
	current := terms[0]
	for i := 1; i < len(terms); i++ {
		current = op("go:seq", current, terms[i])
	}
	return current
}

func formulaValue(f ir.IrFormula) (any, error) {
	return claim_envelope.FormulaToValue(f)
}

func sortValue(t types.Type) any {
	return primitiveSortValue(sortName(t))
}

func primitiveSortValue(name string) any {
	return map[string]any{"kind": "primitive", "name": name}
}

func irSort(t types.Type) ir.Sort {
	if t == nil {
		return ir.Ref
	}
	if b, ok := t.Underlying().(*types.Basic); ok {
		switch {
		case b.Info()&types.IsBoolean != 0:
			return ir.Bool
		case b.Info()&types.IsInteger != 0:
			return ir.Int
		case b.Info()&types.IsFloat != 0:
			return ir.Real
		case b.Info()&types.IsString != 0:
			return ir.String
		}
	}
	return ir.Ref
}

func sortName(t types.Type) string {
	if t == nil {
		return "Ref"
	}
	if b, ok := t.Underlying().(*types.Basic); ok {
		switch {
		case b.Info()&types.IsBoolean != 0:
			return "Bool"
		case b.Info()&types.IsInteger != 0:
			return "Int"
		case b.Info()&types.IsFloat != 0:
			return "Real"
		case b.Info()&types.IsString != 0:
			return "String"
		}
	}
	return "Ref"
}

func typeOfExpr(info *types.Info, expr ast.Expr) types.Type {
	if tv, ok := info.Types[expr]; ok {
		return tv.Type
	}
	return nil
}

func objectCell(obj types.Object) string {
	if obj.Pkg() == nil {
		return obj.Name()
	}
	return obj.Pkg().Path() + "." + obj.Name()
}

func receiverTypeResolved(info *types.Info, fn *ast.FuncDecl) bool {
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return true
	}
	receiver := fn.Recv.List[0].Type
	t := typeOfExpr(info, receiver)
	if ptr, ok := t.(*types.Pointer); ok {
		t = ptr.Elem()
	}
	named, ok := t.(*types.Named)
	return ok && named.Obj() != nil
}

func unresolvedReceiverFunctionName(sourcePath string, pos token.Position, fn *ast.FuncDecl) string {
	name := fn.Name.Name
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return name
	}
	return fmt.Sprintf("%s:%d:%d:%s", sourcePath, pos.Line, pos.Column, name)
}

func fallbackFuncName(packagePath string, fn *ast.FuncDecl) string {
	name := fn.Name.Name
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		recv := exprTypeString(fn.Recv.List[0].Type)
		name = "(" + packagePath + "." + recv + ")." + name
	} else {
		name = packagePath + "." + name
	}
	return name
}

func exprTypeString(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.Ident:
		return e.Name
	case *ast.StarExpr:
		return "*" + exprTypeString(e.X)
	case *ast.SelectorExpr:
		return exprTypeString(e.X) + "." + e.Sel.Name
	default:
		return "Recv"
	}
}

func exprString(fset *token.FileSet, expr ast.Expr) string {
	var b bytes.Buffer
	if err := printer.Fprint(&b, fset, expr); err != nil {
		return "<expr>"
	}
	return b.String()
}

func isPureBuiltin(name string) bool {
	switch name {
	case "len", "cap", "make", "new", "append", "copy", "delete", "complex", "real", "imag":
		return true
	default:
		return false
	}
}

func isIOCall(name string) bool {
	return strings.HasPrefix(name, "fmt.Print") ||
		strings.HasPrefix(name, "fmt.Fprint") ||
		strings.HasPrefix(name, "fmt.Scan") ||
		strings.HasPrefix(name, "os.") ||
		strings.HasPrefix(name, "net.") ||
		strings.Contains(strings.ToLower(name), ".file.") ||
		strings.HasPrefix(name, "io.")
}

func LiftPaths(workspaceRoot string, sourcePaths []string) (LiftResult, error) {
	return LiftPathsWithOptions(workspaceRoot, sourcePaths, LiftOptions{})
}

// LiftPathsCore lifts the given Go sources in the VERIFY-FACING dialect
// (core-arith op symbols). See LiftSourceCore.
func LiftPathsCore(workspaceRoot string, sourcePaths []string) (LiftResult, error) {
	return LiftPathsWithOptions(workspaceRoot, sourcePaths, LiftOptions{NormalizeCoreArith: true})
}

// LiftPathsWithOptions lifts the given Go sources under the given dialect.
func LiftPathsWithOptions(workspaceRoot string, sourcePaths []string, opts LiftOptions) (LiftResult, error) {
	modulePath := modulePathFor(workspaceRoot)
	var merged LiftResult
	for _, sourcePath := range sourcePaths {
		path := sourcePath
		if !filepath.IsAbs(path) {
			path = filepath.Join(workspaceRoot, sourcePath)
		}
		bytes, err := readFile(path)
		if err != nil {
			return LiftResult{}, err
		}
		pkgPath := packagePathFor(modulePath, workspaceRoot, path)
		lifted, err := LiftSourceWithOptions(pkgPath, sourcePath, bytes, opts)
		if err != nil {
			return LiftResult{}, err
		}
		merged.IR = append(merged.IR, lifted.IR...)
		merged.Contracts = append(merged.Contracts, lifted.Contracts...)
		merged.SourceUnits = append(merged.SourceUnits, lifted.SourceUnits...)
		merged.Refusals = append(merged.Refusals, lifted.Refusals...)
		merged.Diagnostics = append(merged.Diagnostics, lifted.Diagnostics...)
		for fnName, ann := range lifted.Annotations {
			if merged.Annotations == nil {
				merged.Annotations = map[string]*Annotation{}
			}
			merged.Annotations[fnName] = ann
		}
	}
	return merged, nil
}
