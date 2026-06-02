package liftgo

const KitDeclarationRPCMethod = "provekit.plugin.kit_declaration"

type KitDeclarationKit struct {
	ID       string `json:"id"`
	Language string `json:"language"`
	Version  string `json:"version"`
}

type KitDeclarationMethod struct {
	Name     string `json:"name"`
	Required bool   `json:"required"`
}

type KitDeclarationRPC struct {
	Methods []KitDeclarationMethod `json:"methods"`
}

type KitDeclarationProofResolution struct {
	Strategy string `json:"strategy"`
}

type KitDeclarationEffectLeaf struct {
	Surface string `json:"surface"`
	Local   string `json:"local"`
	Concept string `json:"concept"`
}

type KitDeclaration struct {
	Kit               KitDeclarationKit             `json:"kit"`
	RPC               KitDeclarationRPC             `json:"rpc"`
	ProofResolution   KitDeclarationProofResolution `json:"proofResolution"`
	EffectKinds       []string                      `json:"effectKinds"`
	EffectLeaves      []KitDeclarationEffectLeaf    `json:"effectLeaves"`
	GuardPredicates   []any                         `json:"guardPredicates"`
	ControlCarriers   []any                         `json:"controlCarriers"`
	ResidueCategories []any                         `json:"residueCategories"`
}

func KitDeclarationResult() KitDeclaration {
	return KitDeclaration{
		Kit: KitDeclarationKit{
			ID:       "go-source",
			Language: "go",
			Version:  Version,
		},
		RPC: KitDeclarationRPC{
			Methods: []KitDeclarationMethod{
				{Name: "initialize", Required: true},
				{Name: KitDeclarationRPCMethod, Required: true},
				{Name: "lift", Required: true},
				{Name: "provekit.plugin.lift_implications", Required: false},
				{Name: "compile", Required: false},
				{Name: "provekit.plugin.recognize", Required: false},
				{Name: "shutdown", Required: false},
			},
		},
		ProofResolution: KitDeclarationProofResolution{Strategy: "go-mod"},
		EffectKinds:     []string{panicFreedomConceptName},
		EffectLeaves: []KitDeclarationEffectLeaf{
			{
				Surface: "go-source",
				Local:   "go:panic",
				Concept: runtimeFailureSiteConceptID,
			},
		},
		GuardPredicates:   []any{},
		ControlCarriers:   []any{},
		ResidueCategories: []any{},
	}
}
