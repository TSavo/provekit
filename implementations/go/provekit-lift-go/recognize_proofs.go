package liftgo

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
)

type listedGoModule struct {
	Dir     string          `json:"Dir"`
	Main    bool            `json:"Main"`
	Replace *listedGoModule `json:"Replace"`
}

var dependencyProofName = regexp.MustCompile(`^blake3-512:[0-9a-f]{128}\.proof$`)

func loadBindingTemplatesForProject(projectRoot string) ([]BindingTemplate, error) {
	paths, err := resolveDependencyProofPaths(projectRoot)
	if err != nil {
		return nil, err
	}
	bindings := make([]BindingTemplate, 0)
	for _, path := range paths {
		proofBindings, err := bindingTemplatesFromProof(path)
		if err != nil {
			return nil, err
		}
		bindings = append(bindings, proofBindings...)
	}
	return bindings, nil
}

func bindingTemplatesFromProof(path string) ([]BindingTemplate, error) {
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
	bindings := make([]BindingTemplate, 0)
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
		binding, ok := bindingTemplateFromSugarEntry(body)
		if ok {
			bindings = append(bindings, binding)
		}
	}
	return bindings, nil
}

func bindingTemplateFromSugarEntry(entry map[string]any) (BindingTemplate, bool) {
	if entry["kind"] != "library-sugar-binding-entry" {
		return BindingTemplate{}, false
	}
	conceptName, ok := entry["concept_name"].(string)
	if !ok || conceptName == "" {
		return BindingTemplate{}, false
	}
	libraryTag, _ := entry["target_library_tag"].(string)
	bodySource, _ := entry["body_source"].(map[string]any)
	template, ok := bodySource["ast_template"]
	if !ok || template == nil {
		return BindingTemplate{}, false
	}
	templateBytes, err := marshalJSONNoHTML(template)
	if err != nil {
		return BindingTemplate{}, false
	}
	templateCID, _ := bodySource["template_cid"].(string)
	if templateCID == "" {
		templateCID = canonicalizer.ComputeCID(templateBytes)
	}
	paramNames := stringSlice(bodySource["param_names"])
	if len(paramNames) == 0 {
		paramNames = stringSlice(entry["param_names"])
	}
	contractCID, _ := entry["contract_cid"].(string)
	return BindingTemplate{
		ConceptName: conceptName,
		LibraryTag:  libraryTag,
		Family:      entry["family"],
		ASTTemplate: json.RawMessage(templateBytes),
		TemplateCID: templateCID,
		ParamNames:  paramNames,
		ContractCID: contractCID,
	}, true
}

func resolveDependencyProofPaths(projectRoot string) ([]string, error) {
	root := strings.TrimSpace(projectRoot)
	if root == "" {
		wd, err := os.Getwd()
		if err != nil {
			return nil, err
		}
		root = wd
	}
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	absRoot = filepath.Clean(absRoot)
	if _, err := os.Stat(filepath.Join(absRoot, "go.mod")); err != nil {
		if os.IsNotExist(err) {
			return []string{}, nil
		}
		return nil, err
	}

	cmd := exec.Command("go", "list", "-m", "-json", "all")
	cmd.Dir = absRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		detail := strings.TrimSpace(string(output))
		if detail != "" {
			return nil, fmt.Errorf("go list -m -json all failed: %w: %s", err, detail)
		}
		return nil, fmt.Errorf("go list -m -json all failed: %w", err)
	}

	proofs := map[string]struct{}{}
	decoder := json.NewDecoder(bytes.NewReader(output))
	for {
		var module listedGoModule
		if err := decoder.Decode(&module); err != nil {
			if err == io.EOF {
				break
			}
			return nil, fmt.Errorf("decode go list module: %w", err)
		}
		if module.Main {
			continue
		}
		dir := effectiveModuleDir(module)
		if dir == "" {
			continue
		}
		if !filepath.IsAbs(dir) {
			dir = filepath.Join(absRoot, dir)
		}
		if err := collectProofPaths(filepath.Clean(dir), proofs); err != nil {
			return nil, err
		}
	}

	paths := make([]string, 0, len(proofs))
	for proof := range proofs {
		paths = append(paths, proof)
	}
	sort.Strings(paths)
	return paths, nil
}

func effectiveModuleDir(module listedGoModule) string {
	if module.Dir != "" {
		return module.Dir
	}
	if module.Replace != nil {
		return effectiveModuleDir(*module.Replace)
	}
	return ""
}

func collectProofPaths(root string, proofs map[string]struct{}) error {
	info, err := os.Stat(root)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if !info.IsDir() {
		return nil
	}
	return filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			switch entry.Name() {
			case ".git", "vendor":
				if path != root {
					return filepath.SkipDir
				}
			}
			return nil
		}
		if !dependencyProofName.MatchString(entry.Name()) {
			return nil
		}
		abs, err := filepath.Abs(path)
		if err != nil {
			return err
		}
		proofs[filepath.Clean(abs)] = struct{}{}
		return nil
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
