package liftgo

import (
	"go/ast"
	"go/parser"
	"go/token"
	"reflect"
	"strconv"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

type SourceSpan struct {
	StartLine int `json:"start_line"`
	StartCol  int `json:"start_col"`
	EndLine   int `json:"end_line"`
	EndCol    int `json:"end_col"`
}

type SugarBodySource struct {
	File        string     `json:"file"`
	Span        SourceSpan `json:"span"`
	SourceCID   string     `json:"source_cid"`
	BodyText    string     `json:"body_text"`
	ASTTemplate any        `json:"ast_template"`
	TemplateCID string     `json:"template_cid"`
	ParamNames  []string   `json:"param_names"`
}

type blockNode struct {
	Kind  string `json:"kind"`
	Stmts []any  `json:"stmts"`
}

type letNode struct {
	Kind string `json:"kind"`
	Pat  any    `json:"pat"`
	Init any    `json:"init"`
}

type exprStmtNode struct {
	Kind         string `json:"kind"`
	Expr         any    `json:"expr"`
	TrailingSemi bool   `json:"trailing_semi"`
}

type callNode struct {
	Kind string `json:"kind"`
	Func any    `json:"func"`
	Args []any  `json:"args"`
}

type methodCallNode struct {
	Kind     string `json:"kind"`
	Receiver any    `json:"receiver"`
	Method   string `json:"method"`
	Args     []any  `json:"args"`
}

type identNode struct {
	Kind string `json:"kind"`
	Name string `json:"name"`
}

type paramRefNode struct {
	Kind  string `json:"kind"`
	Index int    `json:"index"`
}

type pathNode struct {
	Kind     string   `json:"kind"`
	Segments []string `json:"segments"`
}

type litNode struct {
	Kind  string `json:"kind"`
	Ty    string `json:"ty"`
	Value any    `json:"value"`
}

type refNode struct {
	Kind       string `json:"kind"`
	Mutability bool   `json:"mutability"`
	Expr       any    `json:"expr"`
}

type returnNode struct {
	Kind string `json:"kind"`
	Expr any    `json:"expr"`
}

type tupleNode struct {
	Kind  string `json:"kind"`
	Elems []any  `json:"elems"`
}

type arrayNode struct {
	Kind  string `json:"kind"`
	Elems []any  `json:"elems"`
}

type binaryNode struct {
	Kind  string `json:"kind"`
	Op    string `json:"op"`
	Left  any    `json:"left"`
	Right any    `json:"right"`
}

type bindingNode struct {
	Kind string `json:"kind"`
	Name string `json:"name"`
}

type wildcardNode struct {
	Kind string `json:"kind"`
}

type patTupleNode struct {
	Kind  string `json:"kind"`
	Elems []any  `json:"elems"`
}

type otherNode struct {
	Kind    string `json:"kind"`
	Variant string `json:"variant"`
}

func SugarBodySourceForFunc(sourcePath string, source []byte, fnName string) (SugarBodySource, bool, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, source, parser.ParseComments)
	if err != nil {
		return SugarBodySource{}, false, err
	}
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Name.Name != fnName {
			continue
		}
		if fn.Body == nil {
			return SugarBodySource{}, false, nil
		}
		body, err := sugarBodySourceForDecl(fset, sourcePath, source, fn)
		return body, true, err
	}
	return SugarBodySource{}, false, nil
}

func GoFuncBodyText(source []byte, fnName string) (string, bool, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "", source, parser.ParseComments)
	if err != nil {
		return "", false, err
	}
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Name.Name != fnName || fn.Body == nil {
			continue
		}
		return funcBodyText(fset, source, fn), true, nil
	}
	return "", false, nil
}

func sugarBodySourceForDecl(fset *token.FileSet, sourcePath string, source []byte, fn *ast.FuncDecl) (SugarBodySource, error) {
	bodyText := funcBodyText(fset, source, fn)
	paramNames := funcParamNames(fn)
	template := blockToASTTemplate(fn.Body, paramNames)
	templateBytes, err := marshalJSONNoHTML(template)
	if err != nil {
		return SugarBodySource{}, err
	}
	return SugarBodySource{
		File:        sourcePath,
		Span:        funcSpan(fset, fn),
		SourceCID:   canonicalizer.ComputeCID([]byte(bodyText)),
		BodyText:    bodyText,
		ASTTemplate: template,
		TemplateCID: canonicalizer.ComputeCID(templateBytes),
		ParamNames:  paramNames,
	}, nil
}

func funcBodyText(fset *token.FileSet, source []byte, fn *ast.FuncDecl) string {
	if fn == nil || fn.Body == nil {
		return ""
	}
	open := fset.PositionFor(fn.Body.Lbrace, false).Offset
	close := fset.PositionFor(fn.Body.Rbrace, false).Offset
	if open < 0 || close < 0 || open >= close || close > len(source) {
		return ""
	}
	return strings.TrimSpace(string(source[open+1 : close]))
}

func funcSpan(fset *token.FileSet, fn *ast.FuncDecl) SourceSpan {
	start := fset.PositionFor(fn.Pos(), false)
	end := fset.PositionFor(fn.Body.Rbrace, false)
	startCol := start.Column - 1
	if startCol < 0 {
		startCol = 0
	}
	endCol := end.Column
	if endCol < 0 {
		endCol = 0
	}
	return SourceSpan{
		StartLine: start.Line,
		StartCol:  startCol,
		EndLine:   end.Line,
		EndCol:    endCol,
	}
}

func funcParamNames(fn *ast.FuncDecl) []string {
	var names []string
	if fn == nil || fn.Type == nil {
		return names
	}
	if fn.Recv != nil {
		names = append(names, fieldListNames(fn.Recv.List)...)
	}
	if fn.Type.Params != nil {
		names = append(names, fieldListNames(fn.Type.Params.List)...)
	}
	return names
}

func fieldListNames(fields []*ast.Field) []string {
	var names []string
	for _, field := range fields {
		for _, name := range field.Names {
			names = append(names, name.Name)
		}
	}
	return names
}

func blockToASTTemplate(block *ast.BlockStmt, params []string) any {
	stmts := []any{}
	if block != nil {
		for _, stmt := range block.List {
			stmts = append(stmts, stmtToTemplates(stmt, params)...)
		}
	}
	return blockNode{Kind: "block", Stmts: stmts}
}

func stmtToTemplates(stmt ast.Stmt, params []string) []any {
	switch s := stmt.(type) {
	case *ast.AssignStmt:
		if s.Tok != token.ASSIGN && s.Tok != token.DEFINE {
			return []any{otherTemplate(s)}
		}
		out := make([]any, 0, len(s.Lhs))
		for i, lhs := range s.Lhs {
			var init any
			if i < len(s.Rhs) {
				init = exprToTemplate(s.Rhs[i], params)
			}
			out = append(out, letNode{
				Kind: "let",
				Pat:  patToTemplate(lhs, params),
				Init: init,
			})
		}
		return out
	case *ast.DeclStmt:
		return declStmtToTemplates(s, params)
	case *ast.ExprStmt:
		return []any{exprStmtNode{Kind: "expr_stmt", Expr: exprToTemplate(s.X, params), TrailingSemi: true}}
	case *ast.ReturnStmt:
		return []any{returnNode{Kind: "return", Expr: returnExprTemplate(s.Results, params)}}
	case *ast.BlockStmt:
		return []any{blockToASTTemplate(s, params)}
	default:
		return []any{otherTemplate(s)}
	}
}

func declStmtToTemplates(stmt *ast.DeclStmt, params []string) []any {
	gen, ok := stmt.Decl.(*ast.GenDecl)
	if !ok || gen.Tok != token.VAR {
		return []any{otherTemplate(stmt)}
	}
	var out []any
	for _, spec := range gen.Specs {
		valueSpec, ok := spec.(*ast.ValueSpec)
		if !ok {
			out = append(out, otherTemplate(spec))
			continue
		}
		for i, name := range valueSpec.Names {
			var init any
			if i < len(valueSpec.Values) {
				init = exprToTemplate(valueSpec.Values[i], params)
			}
			out = append(out, letNode{
				Kind: "let",
				Pat:  patToTemplate(name, params),
				Init: init,
			})
		}
	}
	return out
}

func returnExprTemplate(results []ast.Expr, params []string) any {
	switch len(results) {
	case 0:
		return nil
	case 1:
		return exprToTemplate(results[0], params)
	default:
		elems := make([]any, 0, len(results))
		for _, result := range results {
			elems = append(elems, exprToTemplate(result, params))
		}
		return tupleNode{Kind: "tuple", Elems: elems}
	}
}

func exprToTemplate(expr ast.Expr, params []string) any {
	switch e := expr.(type) {
	case *ast.BasicLit:
		return litToTemplate(e)
	case *ast.Ident:
		return identToTemplate(e, params)
	case *ast.BinaryExpr:
		op, ok := templateBinaryOp(e.Op)
		if !ok {
			return otherTemplate(e)
		}
		return binaryNode{
			Kind:  "binary",
			Op:    op,
			Left:  exprToTemplate(e.X, params),
			Right: exprToTemplate(e.Y, params),
		}
	case *ast.UnaryExpr:
		if e.Op == token.AND {
			return refNode{Kind: "ref", Mutability: false, Expr: exprToTemplate(e.X, params)}
		}
		return otherTemplate(e)
	case *ast.ParenExpr:
		return exprToTemplate(e.X, params)
	case *ast.CallExpr:
		args := exprListToTemplates(e.Args, params)
		if sel, ok := e.Fun.(*ast.SelectorExpr); ok {
			return methodCallNode{
				Kind:     "method_call",
				Receiver: exprToTemplate(sel.X, params),
				Method:   sel.Sel.Name,
				Args:     args,
			}
		}
		return callNode{Kind: "call", Func: exprToTemplate(e.Fun, params), Args: args}
	case *ast.SelectorExpr:
		segments, ok := selectorSegments(e)
		if !ok {
			return otherTemplate(e)
		}
		return pathNode{Kind: "path", Segments: segments}
	case *ast.CompositeLit:
		elems := make([]any, 0, len(e.Elts))
		for _, elt := range e.Elts {
			elems = append(elems, exprToTemplate(elt, params))
		}
		return arrayNode{Kind: "array", Elems: elems}
	default:
		return otherTemplate(e)
	}
}

func exprListToTemplates(exprs []ast.Expr, params []string) []any {
	out := make([]any, 0, len(exprs))
	for _, expr := range exprs {
		out = append(out, exprToTemplate(expr, params))
	}
	return out
}

func identToTemplate(id *ast.Ident, params []string) any {
	switch id.Name {
	case "true":
		return litNode{Kind: "lit", Ty: "bool", Value: true}
	case "false":
		return litNode{Kind: "lit", Ty: "bool", Value: false}
	case "nil":
		return litNode{Kind: "lit", Ty: "nil", Value: nil}
	}
	if idx := paramIndex(id.Name, params); idx > 0 {
		return paramRefNode{Kind: "param_ref", Index: idx}
	}
	return identNode{Kind: "ident", Name: id.Name}
}

func litToTemplate(lit *ast.BasicLit) any {
	switch lit.Kind {
	case token.STRING:
		value, err := strconv.Unquote(lit.Value)
		if err != nil {
			return otherTemplate(lit)
		}
		return litNode{Kind: "lit", Ty: "str", Value: value}
	case token.CHAR:
		value, err := strconv.Unquote(lit.Value)
		if err != nil {
			return otherTemplate(lit)
		}
		return litNode{Kind: "lit", Ty: "char", Value: value}
	case token.INT:
		value, err := strconv.ParseInt(lit.Value, 0, 64)
		if err != nil {
			return otherTemplate(lit)
		}
		return litNode{Kind: "lit", Ty: "int", Value: value}
	case token.FLOAT:
		value, err := strconv.ParseFloat(lit.Value, 64)
		if err != nil {
			return otherTemplate(lit)
		}
		return litNode{Kind: "lit", Ty: "float64", Value: value}
	default:
		return otherTemplate(lit)
	}
}

func patToTemplate(expr ast.Expr, params []string) any {
	switch p := expr.(type) {
	case *ast.Ident:
		if p.Name == "_" {
			return wildcardNode{Kind: "wildcard"}
		}
		if idx := paramIndex(p.Name, params); idx > 0 {
			return paramRefNode{Kind: "param_ref", Index: idx}
		}
		return bindingNode{Kind: "binding", Name: p.Name}
	case *ast.ParenExpr:
		return patToTemplate(p.X, params)
	default:
		return otherNode{Kind: "pat_other", Variant: astVariantName(p)}
	}
}

func paramIndex(name string, params []string) int {
	for i, param := range params {
		if param == name {
			return i + 1
		}
	}
	return 0
}

func selectorSegments(sel *ast.SelectorExpr) ([]string, bool) {
	segments := []string{sel.Sel.Name}
	for {
		switch x := sel.X.(type) {
		case *ast.Ident:
			segments = append([]string{x.Name}, segments...)
			return segments, true
		case *ast.SelectorExpr:
			segments = append([]string{x.Sel.Name}, segments...)
			sel = x
		default:
			return nil, false
		}
	}
}

func templateBinaryOp(tok token.Token) (string, bool) {
	switch tok {
	case token.ADD:
		return "Add", true
	case token.SUB:
		return "Sub", true
	case token.MUL:
		return "Mul", true
	case token.QUO:
		return "Div", true
	case token.REM:
		return "Rem", true
	case token.AND:
		return "BitAnd", true
	case token.OR:
		return "BitOr", true
	case token.XOR:
		return "BitXor", true
	case token.SHL:
		return "Shl", true
	case token.SHR:
		return "Shr", true
	case token.LAND:
		return "And", true
	case token.LOR:
		return "Or", true
	case token.EQL:
		return "Eq", true
	case token.LSS:
		return "Lt", true
	case token.LEQ:
		return "Le", true
	case token.NEQ:
		return "Ne", true
	case token.GEQ:
		return "Ge", true
	case token.GTR:
		return "Gt", true
	default:
		return "", false
	}
}

func otherTemplate(v any) any {
	return otherNode{Kind: "other", Variant: astVariantName(v)}
}

func astVariantName(v any) string {
	if v == nil {
		return "nil"
	}
	t := reflect.TypeOf(v)
	for t.Kind() == reflect.Pointer {
		t = t.Elem()
	}
	if t.Name() == "" {
		return t.String()
	}
	return t.Name()
}
