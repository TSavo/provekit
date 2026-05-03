// Opacity-manifest emission for vacuous-true validator tags.
//
// Per protocol/specs/2026-05-02-opacity-manifest-grammar.md, an IR
// producer that emits a tractable placeholder for a position whose
// theory it cannot soundly translate MUST also record that position in
// an OpacityManifest. Lift adapters that surface go-playground/validator
// tags like "email", "url", "phone", "credit_card" sit in this slot:
// the validator runs at runtime, but the IR emission is a kit
// predicate (`kit:<tag>`) with no provable content. The manifest names
// each opaque position by its content-address (BLAKE3-512 over the
// JCS-canonical IR-JSON of the Atomic node) and tags it with reason
// code `kit_predicate_no_semantics`.
//
// The library identity (go-playground/validator + version) is pinned in
// the manifest's `compilerVersion` field; the Opacity record itself
// only carries (positionCid, reasonCode) per spec §2.

package validator

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// CompilerName is the dialect identifier for this lift adapter, written
// to OpacityManifest.compiler. Cross-language byte conformance: the
// C# DataAnnotations lift uses its own distinct name; the manifests
// are not byte-equivalent across adapters by design (they are byte-
// equivalent across runs of the same adapter on the same input).
const CompilerName = "provekit-lift-go-validator"

// CompilerVersion identifies the lift adapter's own version and
// declares the runtime validator surface this adapter targets. The
// validator runtime (go-playground/validator) is NOT a Go-module
// dependency of this adapter — the adapter parses the tag string
// directly — so the "targets:" suffix is documentary, surfacing the
// upstream surface name in the manifest's provenance rather than
// asserting a verifiable module pin. A future revision MAY add
// `go-playground/validator/v10` as a real dep and switch this to its
// `runtime.debug.ReadBuildInfo()` value, in which case the suffix
// becomes a verifiable identity pin.
const CompilerVersion = "1.0.0+targets:go-playground/validator/v10"

// ProtocolVersion identifies the manifest grammar this adapter speaks.
// Per protocol/specs/2026-05-02-opacity-manifest-grammar.md §2.1, the
// `protocolVersion` field MUST be the literal "ir-compiler-protocol/2".
// The opacity-manifest grammar is owned by ir-compiler-protocol/2; lift
// adapters that emit manifest entries adopt that grammar tag.
const ProtocolVersion = "ir-compiler-protocol/2"

// VacuousTrueTags lists the validator tag fragments that lift to a
// vacuous-true kit-predicate Atomic. Each emits `Atomic("kit:<tag>", v)`
// at lift time and an OpacityManifest entry at manifest-emit time.
//
// Sorted for deterministic iteration. The list is closed by adapter
// version; a new vacuous tag is a new adapter version.
var VacuousTrueTags = []string{
	"base64",
	"credit_card",
	"datetime",
	"email",
	"hex",
	"ip",
	"ipv4",
	"ipv6",
	"json",
	"phone",
	"url",
	"uuid",
}

// kitPredicateName returns the canonical kit-predicate name for a
// validator tag. Mirror of the C# DataAnnotations adapter naming so
// the IR `kind:atomic, name:"kit:email"` is byte-identical across
// adapters when both lift the same predicate semantics.
func kitPredicateName(tag string) string {
	return "kit:" + tag
}

// vacuousKitPredicate reports whether a tag fragment is one of the
// vacuous-true validators and returns its kit predicate name.
func vacuousKitPredicate(tag string) (string, bool) {
	for _, t := range VacuousTrueTags {
		if t == tag {
			return kitPredicateName(t), true
		}
	}
	return "", false
}

// Opacity is one entry in an OpacityManifest. Field order in the
// emitted JSON is determined by the project canonicalizer (JCS), not
// by struct-tag ordering: keys are sorted ascending by Unicode code
// point at encode time.
type Opacity struct {
	PositionCid string `json:"positionCid"`
	ReasonCode  string `json:"reasonCode"`
}

// OpacityManifest is the JCS-canonicalizable envelope per
// protocol/specs/2026-05-02-opacity-manifest-grammar.md §2.
type OpacityManifest struct {
	Compiler        string    `json:"compiler"`
	CompilerVersion string    `json:"compilerVersion"`
	Opacities       []Opacity `json:"opacities"`
	ProtocolVersion string    `json:"protocolVersion"`
}

// BuildOpacityManifest scans a declaration set for kit-predicate Atomic
// nodes whose names appear in VacuousTrueTags and produces the
// canonical OpacityManifest. The returned manifest's `opacities` array
// is sorted ascending by positionCid (then reasonCode) per spec §2.3.
//
// Empty declaration sets and declaration sets with no vacuous-true
// kit predicates produce a manifest with `opacities: []` per spec §2.2:
// the envelope is still emitted, signalling conformance.
func BuildOpacityManifest(decls []ir.Declaration) (OpacityManifest, error) {
	type seen struct {
		cid    string
		reason string
	}
	var entries []seen
	dedup := map[string]bool{}

	for _, decl := range decls {
		c, ok := decl.(ir.ContractDeclaration)
		if !ok {
			continue
		}
		if c.Pre == nil {
			continue
		}
		raw, err := json.Marshal(c.Pre)
		if err != nil {
			return OpacityManifest{}, err
		}
		var node interface{}
		if err := json.Unmarshal(raw, &node); err != nil {
			return OpacityManifest{}, err
		}
		err = walkKitAtoms(node, func(atomBytes []byte) error {
			canonical, err := canonicalizeAtomBytes(atomBytes)
			if err != nil {
				return err
			}
			cid := canonicalizer.ComputeCID(canonical)
			key := cid + "|kit_predicate_no_semantics"
			if dedup[key] {
				return nil
			}
			dedup[key] = true
			entries = append(entries, seen{cid: cid, reason: "kit_predicate_no_semantics"})
			return nil
		})
		if err != nil {
			return OpacityManifest{}, err
		}
	}

	sort.Slice(entries, func(i, j int) bool {
		if entries[i].cid != entries[j].cid {
			return entries[i].cid < entries[j].cid
		}
		return entries[i].reason < entries[j].reason
	})

	out := make([]Opacity, len(entries))
	for i, e := range entries {
		out[i] = Opacity{PositionCid: e.cid, ReasonCode: e.reason}
	}
	if out == nil {
		out = []Opacity{}
	}

	return OpacityManifest{
		Compiler:        CompilerName,
		CompilerVersion: CompilerVersion,
		Opacities:       out,
		ProtocolVersion: ProtocolVersion,
	}, nil
}

// walkKitAtoms walks a parsed-JSON IR formula node and invokes fn for
// every Atomic whose `name` has the "kit:" prefix. The atom's bytes
// are passed to fn as a freshly-marshalled JSON form, which the caller
// re-canonicalizes through the project canonicalizer for hashing.
func walkKitAtoms(node interface{}, fn func(atomBytes []byte) error) error {
	switch n := node.(type) {
	case map[string]interface{}:
		kind, _ := n["kind"].(string)
		if kind == "atomic" {
			if name, ok := n["name"].(string); ok && strings.HasPrefix(name, "kit:") {
				atomJSON, err := json.Marshal(n)
				if err != nil {
					return err
				}
				return fn(atomJSON)
			}
			return nil
		}
		// Connectives and quantifiers: descend into their child nodes.
		// `operands` (and/or/not/implies), `body` (forall/exists/choice/lambda).
		if ops, ok := n["operands"].([]interface{}); ok {
			for _, child := range ops {
				if err := walkKitAtoms(child, fn); err != nil {
					return err
				}
			}
		}
		if body, ok := n["body"]; ok {
			if err := walkKitAtoms(body, fn); err != nil {
				return err
			}
		}
	case []interface{}:
		for _, child := range n {
			if err := walkKitAtoms(child, fn); err != nil {
				return err
			}
		}
	}
	return nil
}

// canonicalizeAtomBytes runs the project canonicalizer over already-
// JSON-encoded atom bytes, returning JCS-canonical bytes suitable for
// hashing as a positionCid.
func canonicalizeAtomBytes(b []byte) ([]byte, error) {
	var v interface{}
	if err := json.Unmarshal(b, &v); err != nil {
		return nil, err
	}
	return canonicalizer.NewEncoder().Encode(v)
}

// MarshalManifestJCS returns the JCS-canonical bytes of the manifest,
// ready to be written to <cid>.opacity.json. The bytes are stable
// across runs of the same adapter on the same input.
func (m OpacityManifest) MarshalJCS() ([]byte, error) {
	raw, err := json.Marshal(m)
	if err != nil {
		return nil, err
	}
	var v interface{}
	if err := json.Unmarshal(raw, &v); err != nil {
		return nil, err
	}
	return canonicalizer.NewEncoder().Encode(v)
}
