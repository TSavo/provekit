package realizego

import (
	"encoding/json"
	"fmt"
	"go/format"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

type AssembleRequest struct {
	TargetLang   string             `json:"target_lang"`
	FileBasename string             `json:"file_basename"`
	PackageHint  string             `json:"package_hint"`
	Fragments    []AssembleFragment `json:"fragments"`
}

type AssembleFragment struct {
	ConceptName string   `json:"concept_name"`
	Source      string   `json:"source"`
	Imports     []string `json:"imports"`
	Helpers     []string `json:"helpers"`
}

type AssembleResponse struct {
	Files            []AssembledFile `json:"files"`
	CompileClasspath []string        `json:"compile_classpath"`
}

type AssembledFile struct {
	Path    string `json:"path"`
	Content string `json:"content"`
}

func handleAssemble(id json.RawMessage, raw json.RawMessage) any {
	var req AssembleRequest
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &req); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	response, err := Assemble(req)
	if err != nil {
		return errorResponse(id, -32040, "ASSEMBLE_FAILED: "+err.Error())
	}
	return successResponse(id, response)
}

func Assemble(req AssembleRequest) (AssembleResponse, error) {
	if req.TargetLang != "" && req.TargetLang != "go" {
		return AssembleResponse{}, fmt.Errorf("go assembler cannot assemble target_lang %q", req.TargetLang)
	}
	path := goFilePath(req.FileBasename)
	content, err := assembleGoContent(req)
	if err != nil {
		return AssembleResponse{}, err
	}
	return AssembleResponse{
		Files: []AssembledFile{{
			Path:    path,
			Content: content,
		}},
		CompileClasspath: []string{},
	}, nil
}

func assembleGoContent(req AssembleRequest) (string, error) {
	var b strings.Builder
	fmt.Fprintf(&b, "package %s\n\n", goPackageName(req.PackageHint))

	imports := collectGoImports(req.Fragments)
	if len(imports) == 1 {
		fmt.Fprintf(&b, "import %s\n\n", strconv.Quote(imports[0]))
	} else if len(imports) > 1 {
		b.WriteString("import (\n")
		for _, importPath := range imports {
			fmt.Fprintf(&b, "\t%s\n", strconv.Quote(importPath))
		}
		b.WriteString(")\n\n")
	}

	for _, helper := range collectGoHelpers(req.Fragments) {
		b.WriteString(helper)
		b.WriteString("\n\n")
	}
	for _, fragment := range req.Fragments {
		source := strings.TrimSpace(fragment.Source)
		if source == "" {
			continue
		}
		b.WriteString(source)
		b.WriteString("\n\n")
	}

	formatted, err := format.Source([]byte(b.String()))
	if err != nil {
		return "", fmt.Errorf("format assembled Go: %w", err)
	}
	return string(formatted), nil
}

func collectGoImports(fragments []AssembleFragment) []string {
	seen := map[string]struct{}{}
	for _, fragment := range fragments {
		for _, importPath := range fragment.Imports {
			importPath = normalizeImportPath(importPath)
			if importPath == "" {
				continue
			}
			seen[importPath] = struct{}{}
		}
	}
	out := make([]string, 0, len(seen))
	for importPath := range seen {
		out = append(out, importPath)
	}
	sort.Strings(out)
	return out
}

func collectGoHelpers(fragments []AssembleFragment) []string {
	seen := map[string]struct{}{}
	var out []string
	for _, fragment := range fragments {
		for _, helper := range fragment.Helpers {
			helper = strings.TrimSpace(helper)
			if helper == "" {
				continue
			}
			if _, ok := seen[helper]; ok {
				continue
			}
			seen[helper] = struct{}{}
			out = append(out, helper)
		}
	}
	return out
}

func normalizeImportPath(importPath string) string {
	importPath = strings.TrimSpace(importPath)
	if importPath == "" {
		return ""
	}
	if unquoted, err := strconv.Unquote(importPath); err == nil {
		return unquoted
	}
	return importPath
}

func goFilePath(base string) string {
	base = strings.TrimSpace(base)
	if base == "" {
		base = "materialized"
	}
	base = strings.ReplaceAll(base, "\\", "/")
	base = filepath.Base(base)
	base = strings.TrimSuffix(base, ".go")
	return sanitizeFileStem(base) + ".go"
}

func sanitizeFileStem(stem string) string {
	var b strings.Builder
	for _, r := range stem {
		switch {
		case r >= 'a' && r <= 'z':
			b.WriteRune(r)
		case r >= 'A' && r <= 'Z':
			b.WriteRune(r)
		case r >= '0' && r <= '9':
			b.WriteRune(r)
		case r == '_' || r == '-':
			b.WriteRune(r)
		default:
			b.WriteByte('_')
		}
	}
	out := strings.Trim(b.String(), "_-.")
	if out == "" {
		return "materialized"
	}
	return out
}

func goPackageName(hint string) string {
	hint = strings.TrimSpace(hint)
	if hint == "" {
		return "main"
	}
	hint = strings.ReplaceAll(hint, "\\", "/")
	hint = filepath.Base(hint)

	var b strings.Builder
	for i, r := range hint {
		valid := (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || r == '_' || (i > 0 && r >= '0' && r <= '9')
		if valid {
			b.WriteRune(r)
		} else {
			b.WriteByte('_')
		}
	}
	out := strings.Trim(b.String(), "_")
	if out == "" {
		return "main"
	}
	if out[0] >= '0' && out[0] <= '9' {
		out = "pkg_" + out
	}
	if goKeywords[out] {
		out += "_pkg"
	}
	return out
}

var goKeywords = map[string]bool{
	"break":       true,
	"default":     true,
	"func":        true,
	"interface":   true,
	"select":      true,
	"case":        true,
	"defer":       true,
	"go":          true,
	"map":         true,
	"struct":      true,
	"chan":        true,
	"else":        true,
	"goto":        true,
	"package":     true,
	"switch":      true,
	"const":       true,
	"fallthrough": true,
	"if":          true,
	"range":       true,
	"type":        true,
	"continue":    true,
	"for":         true,
	"import":      true,
	"return":      true,
	"var":         true,
}
