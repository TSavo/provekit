package emitgotesting

import (
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

const (
	pluginKind       = "emit"
	pluginVersion    = "0.1.0"
	provenanceCID    = "blake3-512:provenance-provekit-emit-go-testing-0.1.0"
	zeroSignature    = "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
	zeroSigner       = "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
	declaredAt       = "2026-05-25T00:00:00.000Z"
	pluginSourcePath = "implementations/go/provekit-emit-go-testing"
)

func PluginMemento() map[string]any {
	return map[string]any{
		"envelope": map[string]any{
			"declaredAt": declaredAt,
			"signature":  zeroSignature,
			"signer":     zeroSigner,
		},
		"header":   pluginHeader(),
		"metadata": pluginMetadata(),
	}
}

func pluginContent() map[string]any {
	return map[string]any{
		"name":             "provekit-emit-go-testing",
		"version":          pluginVersion,
		"kind":             pluginKind,
		"target_language":  "go",
		"target_framework": "testing",
		"capabilities": map[string]any{
			"kits":       []any{"go"},
			"emits":      "go-testing-assertions",
			"predicates": stringAnyList(supportedPredicates()),
		},
	}
}

func pluginHeader() map[string]any {
	header := map[string]any{
		"content":           pluginContent(),
		"critical":          false,
		"kind":              pluginKind,
		"protocol_versions": []any{"pep/1.7.0"},
		"provenance_cid":    provenanceCID,
		"schemaVersion":     "1",
		"version":           pluginVersion,
	}
	header["cid"] = computePluginCID(header)
	return header
}

func pluginMetadata() map[string]any {
	return map[string]any{
		"maintainer": "T Savo <evilgenius@nefariousplan.com>",
		"note":       "PEP 1.7.0 Go testing emitter: materializes neutral predicates as native testing package assertions. Mapping is inline Go package knowledge.",
		"source_url": pluginSourcePath,
	}
}

func computePluginCID(header map[string]any) string {
	input := map[string]any{
		"content":           header["content"],
		"critical":          header["critical"],
		"kind":              header["kind"],
		"protocol_versions": header["protocol_versions"],
		"provenance_cid":    header["provenance_cid"],
		"schemaVersion":     header["schemaVersion"],
		"version":           header["version"],
	}
	jcs, err := canonicalizer.EncodeJCS(input)
	if err != nil {
		panic(err)
	}
	return canonicalizer.ComputeCID(jcs)
}

func stringAnyList(values []string) []any {
	out := make([]any, len(values))
	for i, value := range values {
		out[i] = value
	}
	return out
}
