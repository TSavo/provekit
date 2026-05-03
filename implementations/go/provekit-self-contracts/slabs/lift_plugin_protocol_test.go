package slabs

import (
	"strings"
	"testing"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// pinnedLiftPluginBridgesBundleCID freezes the BLAKE3-512 of the
// JCS-canonical bytes of the 11 phase-2 cross-kit bridges. Computed at
// PR-authoring time over the BridgeDeclaration array returned by
// BuildLiftPluginProtocolBridges() with TargetContractCid carrying the
// `pending-go-counterpart:<name>` placeholder (the orchestrator
// rewrites these at mint time; the placeholder shape IS what's frozen
// here so the bridge-list itself is content-addressable independent of
// the transient go bundle's internal CIDs).
//
// Drift in any of the following invalidates this hash:
//   - rust contract memento CID for any of the 11 lift-plugin-protocol
//     contracts (rustContractCID map in lift_plugin_protocol.go)
//   - bridge name spelling (bridge_to_<rust_name> or bridge_to_lift_plugin_*)
//   - go counterpart name spelling (go_lift_plugin_*)
//   - sourceLayer / targetLayer / notes literals
//   - LiftPluginGoTargetProofCIDPlaceholder ("deferred:phase-3-proof-bundle")
//   - declaration order
//   - JCS emitter / Go MarshalJSON emitter
//
// Verified against:
//   - Rust contract CIDs extracted from `cargo run -p
//     provekit-self-contracts --bin mint-self-contracts /tmp/<dir>` at
//     bundle CID
//     blake3-512:60df6322388ff7d9ccd1b9ee9d6457fdfe89a51b3d2d73a34daa013
//     1f65c80543331832eb88920fe514ebb799bc655808f152d36274542a
//     c9879c862a33f3a92 (11-contract bundle with C8)
// Updated when C8 (lift_emits_call_edge_stream) was mirrored into Go.
const pinnedLiftPluginBridgesBundleCID = "blake3-512:14714474052a03a17266ae05241ccca6189e7d02a1d961325704b2e1309ec56d80600b0e3e5110531719fc678f410d05e70275b4862d23a1c2f32513b8fa2d5a"

// TestLiftPluginBridgePairsCount ensures all 11 expected bridge pairs
// are declared exactly once.
func TestLiftPluginBridgePairsCount(t *testing.T) {
	pairs := liftPluginBridgePairs()
	if len(pairs) != 11 {
		t.Fatalf("liftPluginBridgePairs(): want 11 pairs, got %d", len(pairs))
	}

	// Every rust source name must appear in the rustContractCID map.
	for _, p := range pairs {
		if _, ok := rustContractCID[p.rustName]; !ok {
			t.Errorf("rust name %q has no entry in rustContractCID map", p.rustName)
		}
		if !strings.HasPrefix(p.bridgeName, "bridge_to_lift_plugin_") {
			t.Errorf("bridge %q must start with 'bridge_to_lift_plugin_'", p.bridgeName)
		}
		if !strings.HasPrefix(p.goCounterpart, "go_lift_plugin_") {
			t.Errorf("counterpart %q must start with 'go_lift_plugin_'", p.goCounterpart)
		}
	}

	// rustContractCID has exactly 11 entries (one per bridge).
	if len(rustContractCID) != 11 {
		t.Errorf("rustContractCID: want 11 entries, got %d", len(rustContractCID))
	}
}

// TestLiftPluginRustContractCIDsAreBlake3_512 validates that every
// frozen rust contract CID is well-formed: prefixed with "blake3-512:"
// and 128 lowercase hex chars after the prefix.
func TestLiftPluginRustContractCIDsAreBlake3_512(t *testing.T) {
	const wantPrefix = "blake3-512:"
	for name, cid := range rustContractCID {
		if !strings.HasPrefix(cid, wantPrefix) {
			t.Errorf("rust CID for %q missing %q prefix: %s", name, wantPrefix, cid)
			continue
		}
		hex := strings.TrimPrefix(cid, wantPrefix)
		if len(hex) != 128 {
			t.Errorf("rust CID for %q: hex length %d, want 128", name, len(hex))
		}
		for _, c := range hex {
			if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')) {
				t.Errorf("rust CID for %q: non-lowercase-hex char %q", name, c)
				break
			}
		}
	}
}

// TestBuildLiftPluginProtocolBridgesShape validates the structural
// invariants of every BridgeDeclaration returned by
// BuildLiftPluginProtocolBridges. Independent of the JCS pin so a shape
// drift surfaces here with a clear message before the hash test sees it.
func TestBuildLiftPluginProtocolBridgesShape(t *testing.T) {
	bridges := BuildLiftPluginProtocolBridges()
	if len(bridges) != 11 {
		t.Fatalf("BuildLiftPluginProtocolBridges(): want 11 bridges, got %d", len(bridges))
	}

	for _, b := range bridges {
		if b.Name == "" {
			t.Errorf("bridge: empty Name")
		}
		if !strings.HasPrefix(b.Name, "bridge_to_lift_plugin_") {
			t.Errorf("bridge %q: Name must start with 'bridge_to_lift_plugin_'", b.Name)
		}
		if b.SourceLayer != "rust-kit" {
			t.Errorf("bridge %q: SourceLayer = %q, want rust-kit", b.Name, b.SourceLayer)
		}
		if b.TargetLayer != "go-kit" {
			t.Errorf("bridge %q: TargetLayer = %q, want go-kit", b.Name, b.TargetLayer)
		}
		if b.SourceContractCid == "" {
			t.Errorf("bridge %q: SourceContractCid must be set (rust memento CID)", b.Name)
		}
		if !strings.HasPrefix(b.SourceContractCid, "blake3-512:") {
			t.Errorf("bridge %q: SourceContractCid must be blake3-512:; got %q", b.Name, b.SourceContractCid)
		}
		if !strings.HasPrefix(b.TargetContractCid, PendingTargetContractCidPrefix) {
			t.Errorf("bridge %q: TargetContractCid must carry pending placeholder; got %q",
				b.Name, b.TargetContractCid)
		}
		if b.TargetProofCid != LiftPluginGoTargetProofCIDPlaceholder {
			t.Errorf("bridge %q: TargetProofCid = %q, want %q",
				b.Name, b.TargetProofCid, LiftPluginGoTargetProofCIDPlaceholder)
		}
		if b.Notes != "lift-plugin-protocol conformance bridge; phase 2" {
			t.Errorf("bridge %q: Notes = %q, want phase-2 marker", b.Name, b.Notes)
		}
	}
}

// TestLiftPluginBridgesPinnedJCSHash freezes the BLAKE3-512 of the
// JCS-canonical bytes of the BridgeDeclaration array. Drift here is
// load-bearing: any change to a rust contract memento CID, a bridge
// name, layer string, notes, or the placeholder shape ripples through
// to a different hash. The orchestrator's resolved-CID path (where
// TargetContractCid gets rewritten to the real go counterpart memento
// CID) deliberately produces a DIFFERENT hash and is intentionally not
// pinned here: that hash is the go bundle's catalog CID, attested
// separately under .provekit/self-contracts-attestations/go.json.
func TestLiftPluginBridgesPinnedJCSHash(t *testing.T) {
	bridges := BuildLiftPluginProtocolBridges()

	// Convert to the Declaration-array shape that ir.MarshalDeclarations
	// expects.
	decls := make([]ir.Declaration, len(bridges))
	for i, b := range bridges {
		decls[i] = b
	}

	jcsBytes, err := ir.MarshalDeclarations(decls)
	if err != nil {
		t.Fatalf("MarshalDeclarations: %v", err)
	}

	got := canonicalizer.ComputeCID(jcsBytes)
	if got != pinnedLiftPluginBridgesBundleCID {
		t.Fatalf("phase-2 bridges JCS hash drift:\n  pinned: %s\n  actual: %s\n\n"+
			"If this drift is intentional, update pinnedLiftPluginBridgesBundleCID "+
			"and re-sign the go self-contracts attestation in a follow-up.",
			pinnedLiftPluginBridgesBundleCID, got)
	}
}
