// Package claim_envelope mints signed ClaimEnvelopes; the protocol's
// content-addressed memento wrappers. Contract / bridge / implication
// variants live in their own files; this file holds the canonical-input
// + signing machinery shared by all three.
//
// Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
//
//	§"CID construction"; cid = "blake3-512:" + hex(BLAKE3_512(canonical(envelope minus
//	cid + producerSignature))). Full 128 hex chars (no truncation).
//	§"Producer-signature scheme (v1)"; ed25519 sign over the
//	same canonical bytes; emit as "ed25519:" + base64(sig).
//
// v1.1.0 cut: contract memento replaces property memento; bindingHash
// and propertyHash are DERIVED inside the minters (not caller-supplied).
// Every hash uses BLAKE3-512 with the "blake3-512:" prefix; signatures
// use the "ed25519:" prefix. Protocol surface is scorched-earth
// self-identifying.
package claim_envelope

import (
	"crypto/ed25519"
	"encoding/base64"
	"sort"

	"github.com/provekit/ir-symbolic/canonicalizer"
)

// Schema CIDs. Stable values producers must use; mirrors the C++
// reference (mint.cpp). v1.1.0 self-identifying form: full 128 hex
// chars under the "blake3-512:" tag.
const (
	SchemaCIDContract    = "blake3-512:00000000000000000000000000000000000000000000000000000000000000d000000000000000000000000000000000000000000000000000000000000000d0"
	SchemaCIDBridge      = "blake3-512:00000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000c0"
	SchemaCIDImplication = "blake3-512:00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000e0"
)

// Verdict values defined by the protocol.
const (
	VerdictHolds    = "holds"
	VerdictViolated = "violated"
	VerdictDecayed  = "decayed"
	VerdictUndec    = "undecidable"
	VerdictError    = "error"
)

// Ed25519SigPrefix is the only permitted signature tag in v1.1.0.
const Ed25519SigPrefix = "ed25519:"

// Minted is the output of any mint operation: signed envelope bytes
// + the envelope's CID (= "blake3-512:" + full BLAKE3-512 hex of
// the unsigned-canonical bytes).
type Minted struct {
	CanonicalBytes []byte // JCS bytes of the FINAL signed envelope
	CID            string // "blake3-512:" + 128 hex chars
}

// Minter is the stateful envelope builder. Holds the signer + a
// reusable JCS encoder. Reentrant; many concurrent mints are fine.
type Minter struct {
	signer  ed25519.PrivateKey
	encoder *canonicalizer.Encoder
	hasher  *canonicalizer.Hasher
}

// NewMinter binds a signing key. The signer is used for all mints
// produced by this Minter; rotate by constructing a new Minter.
func NewMinter(signer ed25519.PrivateKey) *Minter {
	return &Minter{
		signer:  signer,
		encoder: canonicalizer.NewEncoder(),
		hasher:  canonicalizer.NewHasher(),
	}
}

// envelopeForHashing builds the canonical-input JSON-shape value
// (envelope minus cid + producerSignature). Per universal-claim-
// envelope.md §CID construction, this is what hashes to cid AND what
// ed25519 signs over.
func envelopeForHashing(
	bindingHash, propertyHash, verdict, producedBy, producedAt string,
	inputCIDs []string,
	evidence map[string]interface{},
) map[string]interface{} {
	sorted := append([]string(nil), inputCIDs...)
	sort.Strings(sorted)
	if sorted == nil {
		sorted = []string{}
	}
	inputArr := make([]interface{}, len(sorted))
	for i, c := range sorted {
		inputArr[i] = c
	}
	return map[string]interface{}{
		"schemaVersion": "1",
		"bindingHash":   bindingHash,
		"propertyHash":  propertyHash,
		"verdict":       verdict,
		"producedBy":    producedBy,
		"producedAt":    producedAt,
		"inputCids":     inputArr,
		"evidence":      evidence,
	}
}

// finalize is the shared canonicalize → sign → re-canonicalize pipeline.
// All three Mint* funnels go through here.
//
// v1.1.0:
//   - cid uses ComputeCID (full BLAKE3-512 with "blake3-512:" prefix)
//   - producerSignature uses "ed25519:" + base64(sig) self-identifying form
func (m *Minter) finalize(unsigned map[string]interface{}) (*Minted, error) {
	canonical, err := m.encoder.Encode(unsigned)
	if err != nil {
		return nil, err
	}
	cid := m.hasher.ComputeCID(canonical)
	sig := ed25519.Sign(m.signer, canonical)
	sigStr := Ed25519SigPrefix + base64.StdEncoding.EncodeToString(sig)

	signed := make(map[string]interface{}, len(unsigned)+2)
	for k, v := range unsigned {
		signed[k] = v
	}
	signed["producerSignature"] = sigStr
	signed["cid"] = cid

	finalBytes, err := m.encoder.Encode(signed)
	if err != nil {
		return nil, err
	}
	return &Minted{CanonicalBytes: finalBytes, CID: cid}, nil
}

// hashValue returns ComputeCID(JCS(v)); the v1.1.0 protocol's standard
// content-address used for preHash/postHash/invHash, propertyHash,
// bindingHash. v MUST be a JSON-shape value (string, number, bool, nil,
// []interface{}, map[string]interface{}); the JCS encoder will reject
// other types.
//
// Output: "blake3-512:" + 128 hex chars. No truncation.
func hashValue(v interface{}) (string, error) {
	bytes, err := canonicalizer.NewEncoder().Encode(v)
	if err != nil {
		return "", err
	}
	return canonicalizer.ComputeCID(bytes), nil
}

// hashRawString returns ComputeCID(raw bytes of s). NO JCS canonicalization.
// Used for derived hashes whose pre-image is a literal string composed
// of other hashes (e.g. bridge propertyHash = ComputeCID("bridge:" || sourceSymbol)).
func hashRawString(s string) string {
	return canonicalizer.ComputeCID([]byte(s))
}
