// Package realizego is the SHIM that supplies the native Go sugar for a
// ProvekIt contract: the Go peer of provekit-realize-python-core.
//
// A contract, lifted to ProofIR (language-neutral), is MATERIALIZED into a Go
// surface by this realize kit. `provekit materialize` (or a direct
// dispatch_realize call) sends a `provekit.plugin.invoke` request naming a
// cross-language `concept_name` plus the function signature; this kit returns
// REAL Go source realizing that concept. Contract in -> Go sugar out.
//
// Body templates come from Go-module-resolved `.proof` catalogs owned by this
// kit. The Rust CLI dispatches over RPC and never reads Go module proof files
// or Go package internals directly.
package realizego

import (
	"fmt"
	"strings"
)

// KitID identifies this realize kit in emitted provenance.
const KitID = "provekit-realize-go-core@0.1.0"

// bodyTemplate is the kit-internal rendering shape loaded from
// `library-sugar-binding-entry` proof members: a concept name, a
// `${paramN}`-placeholder body template, and a signature guard.
type bodyTemplate struct {
	conceptName string
	template    string // Go statement(s); `${paramN}` substituted by argument names.
	minParams   int
	maxParams   int
}

// RealizeRequest mirrors the fields of the dispatcher's PEP 1.7.0 realize
// request (libprovekit core::RealizeRequest) this kit consumes. Unused fields
// are tolerated (the dispatcher serializes the full request).
type RealizeRequest struct {
	Function              string   `json:"function"`
	Params                []string `json:"params"`
	ParamTypes            []string `json:"param_types"`
	ReturnType            string   `json:"return_type"`
	ConceptName           string   `json:"concept_name"`
	Visibility            string   `json:"visibility"`
	TargetLibraryTag      string   `json:"target_library_tag"`
	TargetLibraryTagCamel string   `json:"targetLibraryTag"`
	ProjectRoot           string   `json:"project_root"`
	ProjectRootCamel      string   `json:"projectRoot"`
}

// RealizedSource is the `result` of a realize invoke: the native Go sugar.
type RealizedSource struct {
	Source    string `json:"source"`
	IsStub    bool   `json:"is_stub"`
	Extension string `json:"extension"`
	KitID     string `json:"kit_id"`
}

// MissingTemplateError signals there is no body template for the requested
// concept under the given signature -- the substrate-honest "this kit does
// not cover that concept" refusal (NOT a silent stub).
type MissingTemplateError struct {
	ConceptName string
	NumParams   int
	Detail      string
}

func (e *MissingTemplateError) Error() string {
	return fmt.Sprintf("missing body-template for concept %q (%d params): %s",
		e.ConceptName, e.NumParams, e.Detail)
}

// Realize produces the Go sugar realizing the requested concept for the given
// function signature. Returns *MissingTemplateError when the concept/signature
// is uncovered.
func Realize(req RealizeRequest) (RealizedSource, error) {
	templates, err := loadBodyTemplatesForProject(req.projectRoot(), req.targetLibraryTag())
	if err != nil {
		return RealizedSource{}, err
	}
	tpl, ok := lookupTemplate(templates, req.ConceptName, len(req.Params))
	if !ok {
		return RealizedSource{}, &MissingTemplateError{
			ConceptName: req.ConceptName,
			NumParams:   len(req.Params),
			Detail:      "no supported concept matches this name + param count",
		}
	}
	body, err := substitute(tpl.template, req.Params)
	if err != nil {
		return RealizedSource{}, err
	}
	source := emitGoFunction(req, body)
	return RealizedSource{
		Source:    source,
		IsStub:    false,
		Extension: "go",
		KitID:     KitID,
	}, nil
}

func (r RealizeRequest) targetLibraryTag() string {
	if r.TargetLibraryTag != "" {
		return r.TargetLibraryTag
	}
	return r.TargetLibraryTagCamel
}

func (r RealizeRequest) projectRoot() string {
	if r.ProjectRoot != "" {
		return r.ProjectRoot
	}
	return r.ProjectRootCamel
}

func lookupTemplate(templates []bodyTemplate, concept string, numParams int) (bodyTemplate, bool) {
	for _, t := range templates {
		if t.conceptName != concept {
			continue
		}
		if numParams < t.minParams || numParams > t.maxParams {
			continue
		}
		return t, true
	}
	return bodyTemplate{}, false
}

// substitute replaces `${paramN}` placeholders with the Nth argument name.
func substitute(template string, params []string) (string, error) {
	out := template
	for i, name := range params {
		out = strings.ReplaceAll(out, fmt.Sprintf("${param%d}", i), name)
	}
	if strings.Contains(out, "${param") {
		return "", fmt.Errorf("template references a parameter not provided: %q", out)
	}
	return out, nil
}

// emitGoFunction assembles a real Go function declaration around the realized
// body, reproducing the requested signature. `func <Fn>(<p0> <t0>, ...) <ret> {
// <body> }`.
func emitGoFunction(req RealizeRequest, body string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "func %s(", req.Function)
	for i, name := range req.Params {
		if i > 0 {
			b.WriteString(", ")
		}
		typ := "int"
		if i < len(req.ParamTypes) && req.ParamTypes[i] != "" {
			typ = req.ParamTypes[i]
		}
		fmt.Fprintf(&b, "%s %s", name, typ)
	}
	b.WriteString(")")
	if req.ReturnType != "" {
		fmt.Fprintf(&b, " %s", req.ReturnType)
	}
	b.WriteString(" {\n\t")
	b.WriteString(body)
	b.WriteString("\n}")
	return b.String()
}
