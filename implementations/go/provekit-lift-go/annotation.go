// Go authoring-surface annotations: the idiom by which a Go library AUTHOR
// declares a ProvekIt boundary / sugar on a function, so the library GETS a
// contract the same way rust (`#[provekit::sugar(...)]`) and java authors do.
//
// Go has no attribute syntax, so the idiomatic analog is a comment-pragma
// directive in the function's doc comment, mirroring the established
// `//go:generate` / `//go:build` convention:
//
//	//provekit:boundary(concept="concept:X")
//	//provekit:sugar(concept="concept:X", library="lib", version="1", family="concept:family:Y")
//	func Foo(...) ... { ... }
//
// `go/ast` attaches such a directive (no space after `//`, directly above the
// func) to the func's `Doc` field, so the parser reads `fn.Doc`. The
// authoring surface (the `go-bind` / `go-contracts` plugins in
// `.provekit/config.toml`) lifts ONLY annotated functions: the DECLARATION
// drives emission, and the emitted contract is the same one that discharges
// through the verifier spine and materializes via provekit-realize-go-core.
package liftgo

import (
	"fmt"
	"go/ast"
	"strconv"
	"strings"
)

// AnnotationKind is `boundary` or `sugar` -- the two authoring declarations.
type AnnotationKind string

const (
	AnnotationBoundary AnnotationKind = "boundary"
	AnnotationSugar    AnnotationKind = "sugar"
)

// Annotation is a parsed `//provekit:<kind>(key="value", ...)` directive.
type Annotation struct {
	Kind    AnnotationKind
	Concept string
	Library string
	Version string
	Family  string
	// Raw holds every parsed key=value pair (including the ones promoted to
	// typed fields) so downstream emission can carry through axes this struct
	// does not name explicitly.
	Raw map[string]string
}

const annotationPrefix = "//provekit:"

// parseFuncAnnotation returns the ProvekIt authoring annotation declared in a
// function's doc comment, or (nil, nil) if none is present. It refuses loudly
// (returns an error) on a malformed `//provekit:` directive rather than
// silently ignoring a typo'd declaration -- an author who wrote `//provekit:`
// meant to declare something.
func parseFuncAnnotation(fn *ast.FuncDecl) (*Annotation, error) {
	if fn == nil || fn.Doc == nil {
		return nil, nil
	}
	for _, c := range fn.Doc.List {
		text := strings.TrimSpace(c.Text)
		if !strings.HasPrefix(text, annotationPrefix) {
			continue
		}
		return parseAnnotationDirective(text, fn.Name.Name)
	}
	return nil, nil
}

// ParseFuncAnnotation exposes the Go-native ProvekIt directive parser to peer
// Go kits. Source understanding stays in Go code; the Rust CLI consumes only
// the kit's normalized RPC result.
func ParseFuncAnnotation(fn *ast.FuncDecl) (*Annotation, error) {
	return parseFuncAnnotation(fn)
}

// parseAnnotationDirective parses one `//provekit:<kind>(...)` line.
func parseAnnotationDirective(text, fnName string) (*Annotation, error) {
	rest := strings.TrimPrefix(text, annotationPrefix)
	open := strings.IndexByte(rest, '(')
	if open < 0 || !strings.HasSuffix(rest, ")") {
		return nil, fmt.Errorf("malformed //provekit annotation on %s: expected `kind(key=\"value\", ...)`, got %q", fnName, text)
	}
	kindStr := strings.TrimSpace(rest[:open])
	var kind AnnotationKind
	switch kindStr {
	case string(AnnotationBoundary):
		kind = AnnotationBoundary
	case string(AnnotationSugar):
		kind = AnnotationSugar
	default:
		return nil, fmt.Errorf("unknown //provekit annotation kind %q on %s (expected `boundary` or `sugar`)", kindStr, fnName)
	}

	body := rest[open+1 : len(rest)-1]
	pairs, err := parseKeyValuePairs(body)
	if err != nil {
		return nil, fmt.Errorf("//provekit:%s on %s: %w", kindStr, fnName, err)
	}

	ann := &Annotation{Kind: kind, Raw: pairs}
	ann.Concept = pairs["concept"]
	ann.Library = pairs["library"]
	ann.Version = pairs["version"]
	ann.Family = pairs["family"]
	if ann.Concept == "" {
		return nil, fmt.Errorf("//provekit:%s on %s must declare a non-empty concept", kindStr, fnName)
	}
	return ann, nil
}

// parseKeyValuePairs parses `key="value", key2="value2"` (and bare-list
// values like `loss=[]`, which are tolerated and stored verbatim). Quote-aware
// so commas inside quoted values are not separators.
func parseKeyValuePairs(body string) (map[string]string, error) {
	out := map[string]string{}
	body = strings.TrimSpace(body)
	if body == "" {
		return out, nil
	}
	for _, segment := range splitTopLevelCommas(body) {
		segment = strings.TrimSpace(segment)
		if segment == "" {
			continue
		}
		eq := strings.IndexByte(segment, '=')
		if eq < 0 {
			return nil, fmt.Errorf("expected key=value, got %q", segment)
		}
		key := strings.TrimSpace(segment[:eq])
		val := strings.TrimSpace(segment[eq+1:])
		// Quoted string value.
		if strings.HasPrefix(val, "\"") {
			unq, err := strconv.Unquote(val)
			if err != nil {
				return nil, fmt.Errorf("key %q has malformed quoted value %q", key, val)
			}
			val = unq
		}
		out[key] = val
	}
	return out, nil
}

// splitTopLevelCommas splits on commas not inside quotes or brackets.
func splitTopLevelCommas(s string) []string {
	var parts []string
	var cur strings.Builder
	inQuote := false
	depth := 0
	for _, r := range s {
		switch r {
		case '"':
			inQuote = !inQuote
			cur.WriteRune(r)
		case '[', '(':
			if !inQuote {
				depth++
			}
			cur.WriteRune(r)
		case ']', ')':
			if !inQuote {
				depth--
			}
			cur.WriteRune(r)
		case ',':
			if !inQuote && depth == 0 {
				parts = append(parts, cur.String())
				cur.Reset()
			} else {
				cur.WriteRune(r)
			}
		default:
			cur.WriteRune(r)
		}
	}
	if cur.Len() > 0 {
		parts = append(parts, cur.String())
	}
	return parts
}
