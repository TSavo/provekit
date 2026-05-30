package realizego

import (
	"bytes"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/format"
	"go/parser"
	"go/printer"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	liftgo "github.com/tsavo/provekit/go/provekit-lift-go"
)

type MaterializeSourceRequest struct {
	ProjectRoot        string `json:"project_root"`
	ProjectRootCamel   string `json:"projectRoot"`
	SourceDir          string `json:"source_dir"`
	SourceDirCamel     string `json:"sourceDir"`
	TargetLang         string `json:"target_lang"`
	TargetLangCamel    string `json:"targetLang"`
	TargetLibraryTag   string `json:"target_library_tag"`
	TargetLibraryCamel string `json:"targetLibraryTag"`
}

type MaterializeSourceResponse struct {
	Files            []MaterializedSourceFile `json:"files"`
	CompileClasspath []string                 `json:"compile_classpath"`
}

type MaterializedSourceFile struct {
	Path    string                 `json:"path"`
	Content string                 `json:"content"`
	Receipt sourceTransformReceipt `json:"receipt"`
}

type sourceTransformReceipt struct {
	SchemaVersion    string           `json:"schema_version"`
	SourceLanguage   string           `json:"source_language"`
	SourceLibrary    *string          `json:"source_library"`
	TargetLanguage   string           `json:"target_language"`
	TargetLibrary    string           `json:"target_library"`
	AggregateSummary aggregateSummary `json:"aggregate_summary"`
	SiteWitnesses    []siteWitness    `json:"site_witnesses"`
	LossRecords      []any            `json:"loss_records"`
	RefusalMementos  []any            `json:"refusal_mementos"`
}

type aggregateSummary struct {
	Exact   int `json:"exact"`
	Lossy   int `json:"lossy"`
	Refused int `json:"refused"`
}

type siteWitness struct {
	SourceBindingCID *string `json:"source_binding_cid"`
	TargetBindingCID string  `json:"target_binding_cid"`
	ContractCID      *string `json:"contract_cid,omitempty"`
	ConceptName      string  `json:"concept_name"`
	FunctionName     string  `json:"function_name"`
	OutcomeKind      string  `json:"outcome_kind"`
}

type sourceEdit struct {
	start int
	end   int
	text  string
}

type materializedSite struct {
	conceptName string
	function    string
	source      string
}

func handleMaterializeSource(id json.RawMessage, raw json.RawMessage) any {
	var req MaterializeSourceRequest
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &req); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	response, err := MaterializeSource(req)
	if err != nil {
		return errorResponse(id, -32050, "MATERIALIZE_SOURCE_FAILED: "+err.Error())
	}
	return successResponse(id, response)
}

func MaterializeSource(req MaterializeSourceRequest) (MaterializeSourceResponse, error) {
	if lang := req.targetLang(); lang != "" && lang != "go" {
		return MaterializeSourceResponse{}, fmt.Errorf("go materializer cannot handle target_lang %q", lang)
	}
	projectRoot := req.projectRoot()
	if projectRoot == "" {
		wd, err := os.Getwd()
		if err != nil {
			return MaterializeSourceResponse{}, err
		}
		projectRoot = wd
	}
	sourceDir := req.sourceDir()
	if sourceDir == "" {
		sourceDir = projectRoot
	}
	sourceDir, err := filepath.Abs(sourceDir)
	if err != nil {
		return MaterializeSourceResponse{}, err
	}
	projectRoot, err = filepath.Abs(projectRoot)
	if err != nil {
		return MaterializeSourceResponse{}, err
	}
	targetLibrary := req.targetLibraryTag()

	var files []MaterializedSourceFile
	err = filepath.WalkDir(sourceDir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			switch entry.Name() {
			case ".git", "vendor", "node_modules", "target":
				if path != sourceDir {
					return filepath.SkipDir
				}
			}
			return nil
		}
		if filepath.Ext(path) != ".go" {
			return nil
		}
		file, ok, err := materializeGoFile(projectRoot, sourceDir, path, targetLibrary)
		if err != nil {
			return err
		}
		if ok {
			files = append(files, file)
		}
		return nil
	})
	if err != nil {
		return MaterializeSourceResponse{}, err
	}
	sort.Slice(files, func(i, j int) bool { return files[i].Path < files[j].Path })
	return MaterializeSourceResponse{Files: files, CompileClasspath: []string{}}, nil
}

func (r MaterializeSourceRequest) projectRoot() string {
	if r.ProjectRoot != "" {
		return strings.TrimSpace(r.ProjectRoot)
	}
	return strings.TrimSpace(r.ProjectRootCamel)
}

func (r MaterializeSourceRequest) sourceDir() string {
	if r.SourceDir != "" {
		return strings.TrimSpace(r.SourceDir)
	}
	return strings.TrimSpace(r.SourceDirCamel)
}

func (r MaterializeSourceRequest) targetLang() string {
	if r.TargetLang != "" {
		return strings.TrimSpace(r.TargetLang)
	}
	return strings.TrimSpace(r.TargetLangCamel)
}

func (r MaterializeSourceRequest) targetLibraryTag() string {
	if r.TargetLibraryTag != "" {
		return strings.TrimSpace(r.TargetLibraryTag)
	}
	return strings.TrimSpace(r.TargetLibraryCamel)
}

func materializeGoFile(projectRoot, sourceDir, path, targetLibrary string) (MaterializedSourceFile, bool, error) {
	source, err := os.ReadFile(path)
	if err != nil {
		return MaterializedSourceFile{}, false, err
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, source, parser.ParseComments)
	if err != nil {
		return MaterializedSourceFile{}, false, err
	}

	var edits []sourceEdit
	var sites []materializedSite
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		ann, err := liftgo.ParseFuncAnnotation(fn)
		if err != nil {
			return MaterializedSourceFile{}, false, err
		}
		if ann == nil {
			continue
		}
		library := ann.Library
		if library == "" {
			library = targetLibrary
		}
		if targetLibrary != "" && library != targetLibrary {
			continue
		}
		if fn.Body == nil {
			return MaterializedSourceFile{}, false, fmt.Errorf("%s: annotated function %s has no body", path, fn.Name.Name)
		}
		params, paramTypes, err := goFunctionParams(fn)
		if err != nil {
			return MaterializedSourceFile{}, false, err
		}
		realized, err := Realize(RealizeRequest{
			Function:         fn.Name.Name,
			Params:           params,
			ParamTypes:       paramTypes,
			ReturnType:       goReturnType(fn),
			ConceptName:      ann.Concept,
			TargetLibraryTag: library,
			ProjectRoot:      projectRoot,
		})
		if err != nil {
			return MaterializedSourceFile{}, false, err
		}
		body, err := realizedBodyLiteral(realized.Source)
		if err != nil {
			return MaterializedSourceFile{}, false, err
		}
		start := fset.Position(fn.Body.Pos()).Offset
		end := fset.Position(fn.Body.End()).Offset
		edits = append(edits, sourceEdit{start: start, end: end, text: body})
		edits = append(edits, directiveRemovalEdits(fset, source, fn)...)
		sites = append(sites, materializedSite{
			conceptName: ann.Concept,
			function:    fn.Name.Name,
			source:      realized.Source,
		})
	}
	if len(sites) == 0 {
		return MaterializedSourceFile{}, false, nil
	}

	rewritten, err := applySourceEdits(source, edits)
	if err != nil {
		return MaterializedSourceFile{}, false, err
	}
	formatted, err := format.Source(rewritten)
	if err != nil {
		return MaterializedSourceFile{}, false, fmt.Errorf("format materialized Go %s: %w\n%s", path, err, string(rewritten))
	}
	rel, err := filepath.Rel(sourceDir, path)
	if err != nil {
		rel = filepath.Base(path)
	}
	return MaterializedSourceFile{
		Path:    filepath.ToSlash(rel),
		Content: string(formatted),
		Receipt: receiptForSites(targetLibrary, sites),
	}, true, nil
}

func goFunctionParams(fn *ast.FuncDecl) ([]string, []string, error) {
	var names []string
	var types []string
	if fn.Type.Params == nil {
		return names, types, nil
	}
	nextAnon := 0
	for _, field := range fn.Type.Params.List {
		typ := typeExprString(field.Type)
		if len(field.Names) == 0 {
			names = append(names, fmt.Sprintf("p%d", nextAnon))
			types = append(types, typ)
			nextAnon++
			continue
		}
		for _, name := range field.Names {
			names = append(names, name.Name)
			types = append(types, typ)
		}
	}
	return names, types, nil
}

func goReturnType(fn *ast.FuncDecl) string {
	if fn.Type.Results == nil || len(fn.Type.Results.List) == 0 {
		return ""
	}
	var parts []string
	for _, field := range fn.Type.Results.List {
		typ := typeExprString(field.Type)
		if len(field.Names) == 0 {
			parts = append(parts, typ)
			continue
		}
		for _, name := range field.Names {
			parts = append(parts, strings.TrimSpace(name.Name+" "+typ))
		}
	}
	if len(parts) == 1 {
		return parts[0]
	}
	return "(" + strings.Join(parts, ", ") + ")"
}

func typeExprString(expr ast.Expr) string {
	var b bytes.Buffer
	if err := printer.Fprint(&b, token.NewFileSet(), expr); err != nil {
		return "int"
	}
	return b.String()
}

func realizedBodyLiteral(source string) (string, error) {
	const prefix = "package materialized\n\n"
	full := []byte(prefix + source)
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "realized.go", full, 0)
	if err != nil {
		return "", fmt.Errorf("parse realized Go source: %w", err)
	}
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		start := fset.Position(fn.Body.Pos()).Offset - len(prefix)
		end := fset.Position(fn.Body.End()).Offset - len(prefix)
		if start < 0 || end > len(source) || start >= end {
			return "", fmt.Errorf("realized body offsets out of range")
		}
		return source[start:end], nil
	}
	return "", fmt.Errorf("realized Go source did not contain a function body")
}

func directiveRemovalEdits(fset *token.FileSet, source []byte, fn *ast.FuncDecl) []sourceEdit {
	if fn.Doc == nil {
		return nil
	}
	var edits []sourceEdit
	for _, comment := range fn.Doc.List {
		if !strings.HasPrefix(strings.TrimSpace(comment.Text), "//provekit:") {
			continue
		}
		start := fset.Position(comment.Pos()).Offset
		end := fset.Position(comment.End()).Offset
		start = lineStart(source, start)
		end = lineEnd(source, end)
		edits = append(edits, sourceEdit{start: start, end: end, text: ""})
	}
	return edits
}

func lineStart(source []byte, offset int) int {
	for offset > 0 && source[offset-1] != '\n' {
		offset--
	}
	return offset
}

func lineEnd(source []byte, offset int) int {
	for offset < len(source) && source[offset] != '\n' {
		offset++
	}
	if offset < len(source) {
		offset++
	}
	return offset
}

func applySourceEdits(source []byte, edits []sourceEdit) ([]byte, error) {
	sort.Slice(edits, func(i, j int) bool {
		if edits[i].start == edits[j].start {
			return edits[i].end > edits[j].end
		}
		return edits[i].start > edits[j].start
	})
	out := append([]byte(nil), source...)
	lastStart := len(out) + 1
	for _, edit := range edits {
		if edit.start < 0 || edit.end < edit.start || edit.end > len(out) {
			return nil, fmt.Errorf("invalid source edit range [%d,%d)", edit.start, edit.end)
		}
		if edit.end > lastStart {
			return nil, fmt.Errorf("overlapping source edits")
		}
		next := make([]byte, 0, len(out)-(edit.end-edit.start)+len(edit.text))
		next = append(next, out[:edit.start]...)
		next = append(next, edit.text...)
		next = append(next, out[edit.end:]...)
		out = next
		lastStart = edit.start
	}
	return out, nil
}

func receiptForSites(targetLibrary string, sites []materializedSite) sourceTransformReceipt {
	witnesses := make([]siteWitness, 0, len(sites))
	for _, site := range sites {
		witnesses = append(witnesses, siteWitness{
			TargetBindingCID: canonicalizer.ComputeCID([]byte(site.source)),
			ConceptName:      site.conceptName,
			FunctionName:     site.function,
			OutcomeKind:      "Exact",
		})
	}
	return sourceTransformReceipt{
		SchemaVersion:    "1",
		SourceLanguage:   "go",
		SourceLibrary:    nil,
		TargetLanguage:   "go",
		TargetLibrary:    targetLibrary,
		AggregateSummary: aggregateSummary{Exact: len(sites)},
		SiteWitnesses:    witnesses,
		LossRecords:      []any{},
		RefusalMementos:  []any{},
	}
}
