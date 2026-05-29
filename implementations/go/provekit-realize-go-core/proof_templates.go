package realizego

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
)

var goIdentRE = regexp.MustCompile(`[A-Za-z_][A-Za-z0-9_]*`)

func loadBodyTemplateEntriesForProject(projectRoot, libraryTag string) ([]map[string]any, error) {
	paths, err := resolveDependencyProofPaths(projectRoot)
	if err != nil {
		return nil, err
	}
	entries := make([]map[string]any, 0)
	for _, path := range paths {
		proofEntries, err := entriesFromShimProof(path, libraryTag)
		if err != nil {
			return nil, err
		}
		entries = append(entries, proofEntries...)
	}
	return entries, nil
}

func loadBodyTemplatesForProject(projectRoot, libraryTag string) ([]bodyTemplate, error) {
	entries, err := loadBodyTemplateEntriesForProject(projectRoot, libraryTag)
	if err != nil {
		return nil, err
	}
	templates := make([]bodyTemplate, 0, len(entries))
	for _, entry := range entries {
		template, ok := bodyTemplateFromEntry(entry)
		if ok {
			templates = append(templates, template)
		}
	}
	return templates, nil
}

func entriesFromShimProof(path, libraryTag string) ([]map[string]any, error) {
	bytes, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read Go shim proof %s: %w", path, err)
	}
	catalog, err := proof_envelope.NewCBORDecoder(bytes).DecodeCatalog()
	if err != nil {
		return nil, fmt.Errorf("decode Go shim proof %s: %w", path, err)
	}
	rawMembers, ok := catalog["members"].(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("decode Go shim proof %s: missing members map", path)
	}
	entries := make([]map[string]any, 0)
	for _, rawMember := range rawMembers {
		memberBytes, ok := rawMember.([]byte)
		if !ok {
			continue
		}
		var parsed map[string]any
		if err := json.Unmarshal(memberBytes, &parsed); err != nil {
			continue
		}
		body := parsed
		if rawBody, ok := parsed["body"].(map[string]any); ok {
			body = rawBody
		}
		if body["kind"] != "library-sugar-binding-entry" {
			continue
		}
		if libraryTag != "" && body["target_library_tag"] != libraryTag {
			continue
		}
		entry, ok := bindingEntryToTemplateEntry(body)
		if ok {
			entries = append(entries, entry)
		}
	}
	return entries, nil
}

func bindingEntryToTemplateEntry(decl map[string]any) (map[string]any, bool) {
	conceptName, ok := decl["concept_name"].(string)
	if !ok || conceptName == "" {
		return nil, false
	}
	paramNames := stringSlice(decl["param_names"])
	bodySource, _ := decl["body_source"].(map[string]any)
	bodyText, _ := bodySource["body_text"].(string)
	if bodyText == "" {
		return nil, false
	}
	libraryTag, _ := decl["target_library_tag"].(string)
	arity := len(paramNames)
	entry := map[string]any{
		"concept_name": conceptName,
		"emission_template": map[string]any{
			"kind":     "verbatim",
			"template": substituteShimParamsWithPlaceholders(bodyText, paramNames),
		},
		"loss_record_contribution": map[string]any{
			"form": "literal",
			"value": map[string]any{
				"entries": []any{},
			},
		},
		"signature_guard": map[string]any{
			"min_params": arity,
			"max_params": arity,
		},
		"target_library_tag": libraryTag,
	}
	if loss, ok := decl["loss_record_contribution"]; ok {
		entry["loss_record_contribution"] = loss
	}
	if observed, ok := decl["observed_dimension"].(string); ok {
		entry["observed_dimension"] = observed
	}
	if helpers, ok := decl["file_helpers"]; ok {
		entry["file_helpers"] = helpers
	}
	return entry, true
}

func bodyTemplateFromEntry(entry map[string]any) (bodyTemplate, bool) {
	conceptName, ok := entry["concept_name"].(string)
	if !ok || conceptName == "" {
		return bodyTemplate{}, false
	}
	emission, _ := entry["emission_template"].(map[string]any)
	template, ok := emission["template"].(string)
	if !ok || template == "" {
		return bodyTemplate{}, false
	}
	guard, _ := entry["signature_guard"].(map[string]any)
	minParams, okMin := intValue(guard["min_params"])
	maxParams, okMax := intValue(guard["max_params"])
	if !okMin || !okMax {
		return bodyTemplate{}, false
	}
	return bodyTemplate{
		conceptName: conceptName,
		template:    template,
		minParams:   minParams,
		maxParams:   maxParams,
	}, true
}

func substituteShimParamsWithPlaceholders(bodyText string, paramNames []string) string {
	return goIdentRE.ReplaceAllStringFunc(bodyText, func(ident string) string {
		for i, name := range paramNames {
			if ident == name {
				return fmt.Sprintf("${param%d}", i)
			}
		}
		return ident
	})
}

func stringSlice(raw any) []string {
	items, ok := raw.([]any)
	if !ok {
		return nil
	}
	out := make([]string, 0, len(items))
	for _, item := range items {
		if value, ok := item.(string); ok {
			out = append(out, value)
		}
	}
	return out
}

func intValue(raw any) (int, bool) {
	switch value := raw.(type) {
	case int:
		return value, true
	case float64:
		return int(value), true
	case json.Number:
		i, err := value.Int64()
		return int(i), err == nil
	default:
		return 0, false
	}
}

func projectRootFromParams(params map[string]any) string {
	projectRoot, _ := params["project_root"].(string)
	if projectRoot == "" {
		projectRoot, _ = params["projectRoot"].(string)
	}
	return strings.TrimSpace(projectRoot)
}

func libraryTagFromParams(params map[string]any) string {
	libraryTag, _ := params["target_library_tag"].(string)
	if libraryTag == "" {
		libraryTag, _ = params["targetLibraryTag"].(string)
	}
	if libraryTag == "" {
		libraryTag, _ = params["library_tag"].(string)
	}
	if libraryTag == "" {
		libraryTag, _ = params["libraryTag"].(string)
	}
	return strings.TrimSpace(libraryTag)
}
