package liftgo

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
)

type ContractBinding struct {
	Name        string `json:"name"`
	ContractCID string `json:"contract_cid"`
}

type ImplicationParams struct {
	WorkspaceRoot    string            `json:"workspace_root"`
	SourcePaths      []string          `json:"source_paths"`
	ContractBindings []ContractBinding `json:"contract_bindings"`
}

type ImplicationResult struct {
	Kind        string           `json:"kind"`
	IR          []any            `json:"ir"`
	Diagnostics []map[string]any `json:"diagnostics"`
}

type goCallSite struct {
	Symbol string
	File   string
	Line   int
	Col    int
}

// LiftImplications is the Go-owned consumer surface for
// `provekit.plugin.lift_implications`. It walks Go AST call expressions and
// emits bridge IR for calls whose callee symbol has a producer contract binding.
func LiftImplications(params ImplicationParams) (ImplicationResult, error) {
	root := params.WorkspaceRoot
	if root == "" {
		cwd, err := os.Getwd()
		if err == nil {
			root = cwd
		} else {
			root = "."
		}
	}
	if len(params.SourcePaths) == 0 {
		return ImplicationResult{}, fmt.Errorf("source_paths must be a non-empty array of strings")
	}

	contractsBySymbol := implicationContractsBySymbol(params.ContractBindings)
	files, err := implicationSourceFiles(root, params.SourcePaths)
	if err != nil {
		return ImplicationResult{}, err
	}

	result := ImplicationResult{
		Kind:        "ir-document",
		IR:          []any{},
		Diagnostics: []map[string]any{},
	}
	for _, path := range files {
		src, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		rel := path
		if r, relErr := filepath.Rel(root, path); relErr == nil {
			rel = r
		}
		callsites, err := collectGoCallsites(rel, src)
		if err != nil {
			continue
		}
		for _, cs := range callsites {
			targetCID := contractsBySymbol[cs.Symbol]
			if targetCID == "" {
				result.Diagnostics = append(result.Diagnostics, map[string]any{
					"kind":   "lift-gap",
					"reason": "no-contract-for-callee",
					"callee": cs.Symbol,
					"file":   cs.File,
					"line":   cs.Line,
					"col":    cs.Col,
				})
				continue
			}
			result.IR = append(result.IR, map[string]any{
				"kind":              "bridge",
				"name":              fmt.Sprintf("intra-body:go:%s@%s:%d:%d", cs.Symbol, cs.File, cs.Line, cs.Col),
				"schemaVersion":     "1",
				"sourceContractCid": targetCID,
				"sourceLayer":       "go",
				"sourceSymbol":      cs.Symbol,
				"target": map[string]any{
					"cid":  targetCID,
					"kind": "contract",
				},
				"targetContractCid": targetCID,
				"targetLayer":       "go-contracts",
				"callsite": map[string]any{
					"file":       cs.File,
					"start_line": cs.Line,
					"start_col":  cs.Col,
				},
			})
		}
	}
	return result, nil
}

func implicationContractsBySymbol(bindings []ContractBinding) map[string]string {
	out := map[string]string{}
	for _, binding := range bindings {
		if binding.ContractCID == "" {
			continue
		}
		for _, key := range implicationBindingKeys(binding.Name) {
			if key != "" {
				if _, exists := out[key]; !exists {
					out[key] = binding.ContractCID
				}
			}
		}
	}
	return out
}

func implicationBindingKeys(name string) []string {
	trimmed := strings.TrimSpace(name)
	if trimmed == "" {
		return nil
	}
	beforeSite := strings.Split(trimmed, "@")[0]
	beforeParams := strings.Split(beforeSite, "(")[0]
	simple := beforeParams
	if i := strings.LastIndex(simple, "."); i >= 0 {
		simple = simple[i+1:]
	}
	return []string{trimmed, beforeSite, beforeParams, simple}
}

func implicationSourceFiles(root string, sourcePaths []string) ([]string, error) {
	var files []string
	for _, sourcePath := range sourcePaths {
		path := sourcePath
		if !filepath.IsAbs(path) {
			path = filepath.Join(root, sourcePath)
		}
		expanded, err := goSourceFiles(path)
		if err != nil {
			return nil, err
		}
		files = append(files, expanded...)
	}
	return files, nil
}

func collectGoCallsites(sourcePath string, src []byte) ([]goCallSite, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, src, parser.ParseComments)
	if err != nil {
		return nil, err
	}
	var out []goCallSite
	ast.Inspect(file, func(node ast.Node) bool {
		call, ok := node.(*ast.CallExpr)
		if !ok {
			return true
		}
		symbol := implicationSourceSymbol(call.Fun)
		if symbol == "" || isIgnorableImplicationCallee(symbol) {
			return true
		}
		pos := fset.Position(call.Fun.Pos())
		out = append(out, goCallSite{
			Symbol: symbol,
			File:   sourcePath,
			Line:   pos.Line,
			Col:    pos.Column,
		})
		return true
	})
	return out, nil
}

func implicationSourceSymbol(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.Ident:
		return e.Name
	case *ast.SelectorExpr:
		return e.Sel.Name
	default:
		return ""
	}
}

func isIgnorableImplicationCallee(symbol string) bool {
	if symbol == "panic" || isPureBuiltin(symbol) {
		return true
	}
	switch symbol {
	case "bool", "byte", "rune", "string",
		"int", "int8", "int16", "int32", "int64",
		"uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
		"float32", "float64", "complex64", "complex128":
		return true
	default:
		return false
	}
}
