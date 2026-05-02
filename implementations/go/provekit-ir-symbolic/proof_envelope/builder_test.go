// SPDX-License-Identifier: Apache-2.0
//
// Builder tests. Cross-impl JCS / deterministic-CBOR conformance
// against the Rust reference (provekit-proof-envelope/src/proof.rs):
//
//   * minimal envelope: 7 entries (kind, name, version, members,
//     signer, declaredAt, signature); first byte is 0xA7 (map-head 7).
//   * envelope with binaryCid: 8 entries; first byte is 0xA8 (map-head
//     8). The optional binaryCid field is back-pinning the catalog to
//     the binary it attests, per
//     protocol/specs/2026-04-30-proof-file-format.md (v1.3.0).
//   * empty BinaryCID is omitted entirely (mirrors Rust's
//     Option<String> -> skip-when-None).
//
// These mirror the Rust kit's `build_minimal_proof_round_trips` test
// (provekit-proof-envelope/src/proof.rs:172) so the two impls agree on
// shape at the same anchor point. A full byte-equal cross-impl check
// would require pinning the deterministic ed25519 signature; this is
// the smallest defensible cross-impl conformance assertion that does
// not depend on signature stability.

package proof_envelope

import (
	"strings"
	"testing"
)

func TestBuildMinimalProofMapHead(t *testing.T) {
	members := map[string][]byte{
		"blake3-512:aa": []byte(`{"hello":"world"}`),
	}
	in := &Input{
		Name:       "@x/y",
		Version:    "0.0.1",
		Members:    members,
		SignerCID:  "blake3-512:bb",
		SignerSeed: [32]byte{0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11},
		DeclaredAt: "2026-04-30T00:00:00.000Z",
	}
	b := NewBuilder()
	out, err := b.Build(in)
	if err != nil {
		t.Fatalf("build: %v", err)
	}
	if !strings.HasPrefix(out.FilenameCID, "blake3-512:") {
		t.Errorf("filename CID prefix: got %s", out.FilenameCID)
	}
	if len(out.Bytes) == 0 || out.Bytes[0] != 0xA7 {
		t.Errorf("map head: want 0xA7 (7 entries), got 0x%02X (cross-impl mismatch with Rust)", out.Bytes[0])
	}
}

func TestBuildProofWithBinaryCID(t *testing.T) {
	members := map[string][]byte{
		"blake3-512:aa": []byte(`{"hello":"world"}`),
	}
	binaryCID := "blake3-512:" + strings.Repeat("c", 128)
	in := &Input{
		Name:       "@x/y",
		Version:    "0.0.1",
		BinaryCID:  binaryCID,
		Members:    members,
		SignerCID:  "blake3-512:bb",
		SignerSeed: [32]byte{0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11},
		DeclaredAt: "2026-04-30T00:00:00.000Z",
	}
	b := NewBuilder()
	out, err := b.Build(in)
	if err != nil {
		t.Fatalf("build: %v", err)
	}
	if len(out.Bytes) == 0 || out.Bytes[0] != 0xA8 {
		t.Errorf("map head with binaryCid: want 0xA8 (8 entries), got 0x%02X", out.Bytes[0])
	}
	if !bytesContainsString(out.Bytes, "binaryCid") {
		t.Errorf("binaryCid key not present in catalog bytes")
	}
	if !bytesContainsString(out.Bytes, binaryCID) {
		t.Errorf("binaryCid value not present in catalog bytes")
	}
}

func TestBuildProofWithoutBinaryCIDOmitsField(t *testing.T) {
	members := map[string][]byte{
		"blake3-512:aa": []byte(`{"hello":"world"}`),
	}
	in := &Input{
		Name:       "@x/y",
		Version:    "0.0.1",
		BinaryCID:  "", // explicitly empty
		Members:    members,
		SignerCID:  "blake3-512:bb",
		SignerSeed: [32]byte{0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11},
		DeclaredAt: "2026-04-30T00:00:00.000Z",
	}
	b := NewBuilder()
	out, err := b.Build(in)
	if err != nil {
		t.Fatalf("build: %v", err)
	}
	if out.Bytes[0] != 0xA7 {
		t.Errorf("map head with empty BinaryCID should match minimal: want 0xA7, got 0x%02X", out.Bytes[0])
	}
	if bytesContainsString(out.Bytes, "binaryCid") {
		t.Errorf("binaryCid key MUST be omitted when empty (mirrors Rust Option<String> skip-when-None)")
	}
}

// bytesContainsString reports whether buf contains the bytes of s as
// a contiguous subsequence. CBOR encodes text strings as their raw
// UTF-8 bytes (preceded by a length-encoded head), so a literal byte-
// substring search reliably finds string fields without needing a
// CBOR decoder.
func bytesContainsString(buf []byte, s string) bool {
	return strings.Contains(string(buf), s)
}
