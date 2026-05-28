package liftgo

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

type RecognizeParams struct {
	ProjectRoot      string            `json:"project_root"`
	SourcePaths      []string          `json:"source_paths"`
	BindingTemplates []BindingTemplate `json:"binding_templates"`
}

type BindingTemplate struct {
	ConceptName string          `json:"concept_name"`
	LibraryTag  string          `json:"library_tag"`
	Family      any             `json:"family"`
	ASTTemplate json.RawMessage `json:"ast_template"`
	TemplateCID string          `json:"template_cid"`
	ParamNames  []string        `json:"param_names"`
	ContractCID string          `json:"contract_cid"`
}

type RecognizeResponse struct {
	Tags []RecognizeTag `json:"tags"`
}

type RecognizeTag struct {
	File          string         `json:"file"`
	Span          SourceSpan     `json:"span"`
	FunctionName  string         `json:"function_name"`
	ConceptName   string         `json:"concept_name"`
	LibraryTag    string         `json:"library_tag"`
	Family        any            `json:"family"`
	TemplateCID   string         `json:"template_cid"`
	ContractCID   string         `json:"contract_cid"`
	MatchTier     string         `json:"match_tier"`
	ParamBindings []ParamBinding `json:"param_bindings"`
}

type ParamBinding struct {
	Index      int    `json:"index"`
	SourceText string `json:"source_text"`
}

func RecognizeImpl(params RecognizeParams) (RecognizeResponse, error) {
	if params.ProjectRoot == "" {
		return RecognizeResponse{}, fmt.Errorf("missing `project_root`")
	}
	if params.SourcePaths == nil {
		return RecognizeResponse{}, fmt.Errorf("missing `source_paths` array")
	}

	bindingsByCID := map[string]BindingTemplate{}
	for _, binding := range params.BindingTemplates {
		if binding.TemplateCID == "" {
			continue
		}
		bindingsByCID[binding.TemplateCID] = binding
	}

	tags := []RecognizeTag{}
	for _, relPath := range params.SourcePaths {
		path := relPath
		if !filepath.IsAbs(path) {
			path = filepath.Join(params.ProjectRoot, relPath)
		}
		source, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		fileTags, err := recognizeFile(relPath, source, bindingsByCID)
		if err != nil {
			continue
		}
		tags = append(tags, fileTags...)
	}
	return RecognizeResponse{Tags: tags}, nil
}

func recognizeFile(relPath string, source []byte, bindingsByCID map[string]BindingTemplate) ([]RecognizeTag, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, relPath, source, parser.ParseComments)
	if err != nil {
		return nil, err
	}
	tags := []RecognizeTag{}
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		tag, ok, err := recognizeFunc(fset, relPath, fn, bindingsByCID)
		if err != nil {
			return nil, err
		}
		if ok {
			tags = append(tags, tag)
		}
	}
	return tags, nil
}

func recognizeFunc(fset *token.FileSet, relPath string, fn *ast.FuncDecl, bindingsByCID map[string]BindingTemplate) (RecognizeTag, bool, error) {
	paramNames := funcParamNames(fn)
	template := blockToASTTemplate(fn.Body, paramNames)
	templateBytes, err := marshalJSONNoHTML(template)
	if err != nil {
		return RecognizeTag{}, false, err
	}
	templateCID := canonicalizer.ComputeCID(templateBytes)
	binding, ok := bindingsByCID[templateCID]
	if !ok {
		return RecognizeTag{}, false, nil
	}
	paramBindings := make([]ParamBinding, 0, len(paramNames))
	for i, name := range paramNames {
		paramBindings = append(paramBindings, ParamBinding{
			Index:      i + 1,
			SourceText: name,
		})
	}
	return RecognizeTag{
		File:          relPath,
		Span:          funcSpan(fset, fn),
		FunctionName:  fn.Name.Name,
		ConceptName:   binding.ConceptName,
		LibraryTag:    binding.LibraryTag,
		Family:        binding.Family,
		TemplateCID:   templateCID,
		ContractCID:   binding.ContractCID,
		MatchTier:     "exact",
		ParamBindings: paramBindings,
	}, true, nil
}
