// Package claim_envelope mints signed ClaimEnvelopes; the protocol's
// content-addressed memento wrappers. Contract / bridge / implication
// variants live in their own files; this file holds the canonical-input
// + signing machinery shared by all three.
//
// Spec: protocol/specs/2026-04-29-universal-claim-envelope.md
//       §"CID construction"; cid = sha256(canonical(envelope minus
//       cid + producerSignature))[:32 hex chars].
//       §"Producer-signature scheme (v1)"; ed25519 sign over the
//       same canonical bytes.
//
// v1.1.0 cut: contract memento replaces property memento; bindingHash
// and propertyHash are DERIVED inside the minters (not caller-supplied).
package claim_envelope

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"sort"

	"github.com/provekit/ir-symbolic/canonicalizer"
)

// Schema CIDs. Stable values producers must use; mirrors the C++
// reference and the (eventual) TS port. The protocol cut renamed
// "property" → "contract" and added "implication".
const (
	SchemaCIDContract    = "0000000000000000d0000000000000d0"
	SchemaCIDBridge      = "0000000000000000c0000000000000c0"
	SchemaCIDImplication = "0000000000000000e0000000000000e0"
)

// Verdict values defined by the protocol.
const (
	VerdictHolds    = "holds"
	VerdictViolated = "violated"
	VerdictDecayed  = "decayed"
	VerdictUndec    = "undecidable"
	VerdictError    = "error"
)

// Minted is the output of any mint operation: signed envelope bytes
// + the envelope's CID (= sha256 of the unsigned-canonical-bytes).
type Minted struct {
	CanonicalBytes []byte // JCS bytes of the FINAL signed envelope
	CID            string // 32 lowercase hex chars
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
func (m *Minter) finalize(unsigned map[string]interface{}) (*Minted, error) {
	canonical, err := m.encoder.Encode(unsigned)
	if err != nil {
		return nil, err
	}
	cid := m.hasher.EnvelopeCID32(canonical)
	sig := ed25519.Sign(m.signer, canonical)
	sigB64 := base64.StdEncoding.EncodeToString(sig)

	signed := make(map[string]interface{}, len(unsigned)+2)
	for k, v := range unsigned {
		signed[k] = v
	}
	signed["producerSignature"] = sigB64
	signed["cid"] = cid

	finalBytes, err := m.encoder.Encode(signed)
	if err != nil {
		return nil, err
	}
	return &Minted{CanonicalBytes: finalBytes, CID: cid}, nil
}

// hash16Value returns hash16(JCS(v)); the protocol's standard
// content-address prefix used for preHash/postHash/invHash, propertyHash,
// bindingHash. v MUST be a JSON-shape value (string, number, bool, nil,
// []interface{}, map[string]interface{}); the JCS encoder will reject
// other types.
func hash16Value(v interface{}) (string, error) {
	bytes, err := canonicalizer.NewEncoder().Encode(v)
	if err != nil {
		return "", err
	}
	return canonicalizer.NewHasher().PropertyHash16(bytes), nil
}

// hash16RawString returns hash16(s); sha256(raw bytes of s)[:16 hex],
// NO JCS canonicalization. Used for derived hashes whose pre-image is
// a literal string composed of other hashes (e.g. bridge propertyHash
// = hash16("bridge:" || sourceSymbol)).
func hash16RawString(s string) string {
	sum := sha256.Sum256([]byte(s))
	return hex.EncodeToString(sum[:])[:16]
}
