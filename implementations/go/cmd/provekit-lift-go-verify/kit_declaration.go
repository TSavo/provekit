package main

const kitDeclarationRPCMethodName = "provekit.plugin.kit_declaration"

func kitDeclarationResult() map[string]any {
	return map[string]any{
		"kit": map[string]any{
			"id":       "go",
			"language": "go",
			"version":  "0.1.0",
		},
		"rpc": map[string]any{
			"methods": []any{
				map[string]any{"name": "initialize", "required": true},
				map[string]any{"name": kitDeclarationRPCMethodName, "required": true},
				map[string]any{"name": "lift", "required": true},
				map[string]any{"name": "shutdown", "required": false},
			},
		},
		"proofResolution": map[string]any{"strategy": "go-mod"},
		"effectKinds":     []any{"concept:panic-freedom"},
		"effectLeaves": []any{
			map[string]any{
				"surface": "go",
				"local":   "go:panic",
				"concept": "concept:panic-freedom.leaf.runtime-failure-site",
			},
		},
		"guardPredicates":   []any{},
		"controlCarriers":   []any{},
		"residueCategories": []any{},
	}
}
