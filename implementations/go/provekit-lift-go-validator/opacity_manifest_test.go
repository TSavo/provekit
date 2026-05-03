package validator

import (
	"encoding/json"
	"strings"
	"testing"

	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// Per protocol/specs/2026-05-02-opacity-manifest-grammar.md §2.2,
// every conformant adapter MUST emit a manifest envelope even when
// no positions are opaque. The envelope shape locks the adapter into
// the protocol-version handshake.
func TestBuildOpacityManifest_EmptyDeclarations(t *testing.T) {
	m, err := BuildOpacityManifest(nil)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if m.ProtocolVersion != "ir-compiler-protocol/2" {
		t.Errorf("protocolVersion = %q, want %q", m.ProtocolVersion, "ir-compiler-protocol/2")
	}
	if m.Compiler != "provekit-lift-go-validator" {
		t.Errorf("compiler = %q, want %q", m.Compiler, "provekit-lift-go-validator")
	}
	if m.CompilerVersion == "" {
		t.Error("compilerVersion must be non-empty")
	}
	if !strings.Contains(m.CompilerVersion, "go-playground/validator") {
		t.Errorf("compilerVersion %q must surface the targeted validator library name", m.CompilerVersion)
	}
	if !strings.Contains(m.CompilerVersion, "targets:") {
		t.Errorf("compilerVersion %q must mark the library tag as documentary (targets: prefix)", m.CompilerVersion)
	}
	if len(m.Opacities) != 0 {
		t.Errorf("Opacities = %v, want empty", m.Opacities)
	}
}

// A struct with only sound predicates (no vacuous validators) emits a
// manifest with empty opacities. Spec §2.2: the envelope is mandatory.
func TestBuildOpacityManifest_NoVacuousPredicates(t *testing.T) {
	type Score struct {
		Value int `validate:"gte=0,lte=100"`
	}
	decls := LiftStruct(Score{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 0 {
		t.Errorf("Opacities = %v, want empty for sound predicates", m.Opacities)
	}
}

// Each known vacuous-true validator tag emits exactly one opacity entry
// with reasonCode = "kit_predicate_no_semantics".
func TestBuildOpacityManifest_EachVacuousTagEmitsEntry(t *testing.T) {
	type Email struct {
		V string `validate:"email"`
	}
	type URL struct {
		V string `validate:"url"`
	}
	type Phone struct {
		V string `validate:"phone"`
	}
	type CreditCard struct {
		V string `validate:"credit_card"`
	}

	cases := []struct {
		name  string
		decls []ir.Declaration
	}{
		{"email", LiftStruct(Email{})},
		{"url", LiftStruct(URL{})},
		{"phone", LiftStruct(Phone{})},
		{"credit_card", LiftStruct(CreditCard{})},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m, err := BuildOpacityManifest(tc.decls)
			if err != nil {
				t.Fatalf("BuildOpacityManifest: %v", err)
			}
			if len(m.Opacities) != 1 {
				t.Fatalf("Opacities count = %d, want 1", len(m.Opacities))
			}
			if got := m.Opacities[0].ReasonCode; got != "kit_predicate_no_semantics" {
				t.Errorf("reasonCode = %q, want kit_predicate_no_semantics", got)
			}
			if !strings.HasPrefix(m.Opacities[0].PositionCid, "blake3-512:") {
				t.Errorf("positionCid = %q, want blake3-512: prefix", m.Opacities[0].PositionCid)
			}
		})
	}
}

// A struct that mixes two vacuous validators emits two distinct
// positionCids. This guards against the Predicates.And()-vs-Atomic
// collision: with the pre-task placeholder of `ir.And()` for both
// email and url, two fields would have collapsed to one positionCid.
// With kit:<tag> Atomics, they MUST stay distinct.
func TestBuildOpacityManifest_DistinctPositionsForDistinctValidators(t *testing.T) {
	type Mixed struct {
		Email string `validate:"email"`
		URL   string `validate:"url"`
	}
	decls := LiftStruct(Mixed{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 2 {
		t.Fatalf("Opacities count = %d, want 2", len(m.Opacities))
	}
	if m.Opacities[0].PositionCid == m.Opacities[1].PositionCid {
		t.Errorf("email and url collapsed to one positionCid: %q", m.Opacities[0].PositionCid)
	}
}

// Spec §2.3: opacities array is sorted by positionCid ascending.
func TestBuildOpacityManifest_OpacitiesSortedByPositionCid(t *testing.T) {
	type Many struct {
		A string `validate:"email"`
		B string `validate:"url"`
		C string `validate:"phone"`
		D string `validate:"credit_card"`
	}
	decls := LiftStruct(Many{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 4 {
		t.Fatalf("Opacities count = %d, want 4", len(m.Opacities))
	}
	for i := 1; i < len(m.Opacities); i++ {
		if m.Opacities[i-1].PositionCid > m.Opacities[i].PositionCid {
			t.Errorf("opacities not sorted ascending at i=%d: %q > %q",
				i, m.Opacities[i-1].PositionCid, m.Opacities[i].PositionCid)
		}
	}
}

// A struct that uses the same vacuous validator on two fields produces
// two distinct entries: positionCid hashes the Atomic node including
// its `args`, which differ between fields (different var names).
func TestBuildOpacityManifest_SameValidatorDifferentFields(t *testing.T) {
	type TwoEmails struct {
		Primary   string `validate:"email"`
		Secondary string `validate:"email"`
	}
	decls := LiftStruct(TwoEmails{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 2 {
		t.Fatalf("Opacities count = %d, want 2", len(m.Opacities))
	}
	if m.Opacities[0].PositionCid == m.Opacities[1].PositionCid {
		t.Error("Primary.email and Secondary.email collapsed; positionCid must include the Atomic args (var name)")
	}
}

// A `required,email` combo emits ONE opacity entry: the `required` part
// is a sound `≠` predicate; only the `email` part is opaque. The walker
// must descend into the wrapping `and` connective.
func TestBuildOpacityManifest_AndDescent(t *testing.T) {
	type RequiredEmail struct {
		V string `validate:"required,email"`
	}
	decls := LiftStruct(RequiredEmail{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 1 {
		t.Fatalf("Opacities count = %d, want 1 (only the email atom is opaque)", len(m.Opacities))
	}
}

// Spec §2.1: protocolVersion MUST be the literal "ir-compiler-protocol/2".
// JCS-encoded bytes round-trip identically.
func TestOpacityManifest_MarshalJCS_RoundTrips(t *testing.T) {
	type Sample struct {
		V string `validate:"email"`
	}
	decls := LiftStruct(Sample{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	bytes1, err := m.MarshalJCS()
	if err != nil {
		t.Fatalf("MarshalJCS: %v", err)
	}
	// Decode + re-encode must produce identical bytes (JCS is idempotent
	// on canonical input).
	var v interface{}
	if err := json.Unmarshal(bytes1, &v); err != nil {
		t.Fatalf("Unmarshal: %v", err)
	}
	bytes2, err := m.MarshalJCS()
	if err != nil {
		t.Fatalf("MarshalJCS round 2: %v", err)
	}
	if string(bytes1) != string(bytes2) {
		t.Errorf("JCS not idempotent:\n  first:  %s\n  second: %s", bytes1, bytes2)
	}
	// Spec required-keys present.
	for _, k := range []string{`"protocolVersion":"ir-compiler-protocol/2"`,
		`"compiler":"provekit-lift-go-validator"`,
		`"opacities":`} {
		if !strings.Contains(string(bytes1), k) {
			t.Errorf("manifest JCS missing required key %q in: %s", k, bytes1)
		}
	}
}

// Cross-language byte-conformance pin per
// protocol/specs/2026-05-02-opacity-manifest-grammar.md §6.
//
// Both the Go validator lift (`V string `validate:"email"``) and the
// C# DataAnnotations lift (`[EmailAddress] public string V`) lift to
// the byte-identical IR atom:
//
//   {"args":[{"kind":"var","name":"V"}],"kind":"atomic","name":"kit:email"}
//
// The BLAKE3-512 of the JCS-canonical bytes — the positionCid — MUST
// be identical across languages. This test pins the hash; the C#
// peer test in OpacityManifestTests.cs asserts the same constant.
// Drift in either canonicalizer's output bytes (key order, escape
// rules, UTF-8 handling) breaks one test or the other.
const KitEmailPositionCidPin = "blake3-512:ea31bf7d7052172f05c3254fc2cfb8809daf9f4a9578090ce7c46b35ab5f1d208c16e58a98314a8659dfcae1858165771eafa8639e7522ff2870140933a7cd27"

func TestBuildOpacityManifest_KitEmailGoldenCID_CrossLanguagePin(t *testing.T) {
	type EmailFieldV struct {
		V string `validate:"email"`
	}
	decls := LiftStruct(EmailFieldV{})
	m, err := BuildOpacityManifest(decls)
	if err != nil {
		t.Fatalf("BuildOpacityManifest: %v", err)
	}
	if len(m.Opacities) != 1 {
		t.Fatalf("Opacities count = %d, want 1", len(m.Opacities))
	}
	got := m.Opacities[0].PositionCid
	if got != KitEmailPositionCidPin {
		t.Errorf("kit:email(V) positionCid drifted from cross-language pin\n  got:    %s\n  pinned: %s\n  (the C# DataAnnotations lift asserts the same constant)",
			got, KitEmailPositionCidPin)
	}
}

// VacuousTrueTags must include the four task-spec'd validators.
func TestVacuousTrueTags_CoversTaskSpec(t *testing.T) {
	required := []string{"email", "url", "phone", "credit_card"}
	for _, r := range required {
		found := false
		for _, t := range VacuousTrueTags {
			if t == r {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("VacuousTrueTags missing task-spec validator %q", r)
		}
	}
}
