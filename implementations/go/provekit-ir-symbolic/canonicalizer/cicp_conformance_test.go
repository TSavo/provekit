package canonicalizer

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

type cicpManifest struct {
	Vectors []cicpVector `json:"vectors"`
}

type cicpVector struct {
	Name          string `json:"name"`
	Body          string `json:"body"`
	ExpectedCID   string `json:"expectedCid"`
	ShouldPass    bool   `json:"shouldPass"`
	ErrorContains string `json:"errorContains"`
}

func TestCICPGoldenVectors(t *testing.T) {
	dir := cicpConformanceDir()
	manifest := readCICPManifest(t, filepath.Join(dir, "vectors.json"))

	for _, vector := range manifest.Vectors {
		vector := vector
		t.Run(vector.Name, func(t *testing.T) {
			body := readCICPJSON(t, filepath.Join(dir, vector.Body))

			if !vector.ShouldPass {
				err := validateCICPInputClosure(body)
				if err == nil {
					t.Fatalf("validateCICPInputClosure succeeded; want fail-closed error containing %q", vector.ErrorContains)
				}
				if !strings.Contains(err.Error(), vector.ErrorContains) {
					t.Fatalf("validateCICPInputClosure error = %q, want substring %q", err, vector.ErrorContains)
				}
				return
			}

			if err := validateCICPInputClosure(body); err != nil {
				t.Fatalf("validateCICPInputClosure: %v", err)
			}

			canonical, err := EncodeJCS(body)
			if err != nil {
				t.Fatalf("EncodeJCS(%s): %v", vector.Body, err)
			}
			got := ComputeCID(canonical)
			if got != vector.ExpectedCID {
				t.Fatalf("CID mismatch for %s:\n  got:  %s\n  want: %s", vector.Body, got, vector.ExpectedCID)
			}
		})
	}
}

func cicpConformanceDir() string {
	return filepath.Clean(filepath.Join("..", "..", "..", "..", "protocol", "conformance", "cicp"))
}

func readCICPManifest(t *testing.T, path string) cicpManifest {
	t.Helper()

	var manifest cicpManifest
	readJSONFile(t, path, &manifest)
	if len(manifest.Vectors) == 0 {
		t.Fatalf("%s: no vectors", path)
	}
	return manifest
}

func readCICPJSON(t *testing.T, path string) map[string]interface{} {
	t.Helper()

	var body map[string]interface{}
	readJSONFile(t, path, &body)
	return body
}

func readJSONFile(t *testing.T, path string, into interface{}) {
	t.Helper()

	file, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer file.Close()

	dec := json.NewDecoder(file)
	dec.UseNumber()
	if err := dec.Decode(into); err != nil {
		t.Fatalf("decode %s: %v", path, err)
	}
}

func validateCICPInputClosure(body map[string]interface{}) error {
	inputs, err := stringSetField(body, "inputCids")
	if err != nil {
		return err
	}

	for _, cid := range requiredCICPInputCIDs(body) {
		if _, ok := inputs[cid]; !ok {
			return fmt.Errorf("inputCids missing required CID %s", cid)
		}
	}
	return nil
}

func requiredCICPInputCIDs(body map[string]interface{}) []string {
	switch stringField(body, "kind") {
	case "CIBlastRadius":
		return appendCIDFields(body,
			[]string{
				"protocolCatalogCid",
				"jobDefinitionCid",
				"commandCid",
				"runnerIdentityCid",
				"sourceClosureCid",
				"policyCid",
			},
			[]string{
				"toolchainCids",
				"lockfileCids",
				"generatedInputCids",
				"fixtureCids",
				"relevantSpecCids",
			},
		)
	case "CIJobResultBodyClaim":
		return appendCIDFields(body,
			[]string{
				"blastRadiusCid",
				"outputCid",
				"logCid",
				"runnerIdentityCid",
				"policyCid",
			},
			nil,
		)
	case "CIReuseBodyClaim":
		return appendCIDFields(body,
			[]string{
				"currentBlastRadiusCid",
				"previousBlastRadiusCid",
				"previousResultWitnessCid",
				"policyCid",
			},
			[]string{"bridgeWitnessCids"},
		)
	case "CIImpactBodyClaim":
		return appendCIDFields(body,
			[]string{
				"baseStateCid",
				"candidateStateCid",
				"policyCid",
			},
			[]string{
				"protocolEvolutionWitnessCids",
				"changedBlastRadiusCids",
				"unchangedBlastRadiusCids",
				"reusableWitnessCids",
				"refusalCids",
			},
		)
	default:
		return nil
	}
}

func appendCIDFields(body map[string]interface{}, scalarFields, listFields []string) []string {
	var cids []string
	for _, field := range scalarFields {
		if cid := stringField(body, field); cid != "" {
			cids = append(cids, cid)
		}
	}
	for _, field := range listFields {
		cids = append(cids, stringSliceField(body, field)...)
	}
	return cids
}

func stringField(body map[string]interface{}, field string) string {
	value, _ := body[field].(string)
	return value
}

func stringSliceField(body map[string]interface{}, field string) []string {
	values, ok := body[field].([]interface{})
	if !ok {
		return nil
	}
	out := make([]string, 0, len(values))
	for _, value := range values {
		if s, ok := value.(string); ok {
			out = append(out, s)
		}
	}
	return out
}

func stringSetField(body map[string]interface{}, field string) (map[string]struct{}, error) {
	values, ok := body[field].([]interface{})
	if !ok {
		return nil, fmt.Errorf("%s must be an array", field)
	}

	out := make(map[string]struct{}, len(values))
	for i, value := range values {
		s, ok := value.(string)
		if !ok {
			return nil, fmt.Errorf("%s[%d] must be a string", field, i)
		}
		out[s] = struct{}{}
	}
	return out, nil
}
