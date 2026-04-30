package proof_envelope

import (
	"bytes"
	"crypto/ed25519"
	"sort"

	"github.com/provekit/ir-symbolic/canonicalizer"
)

// Builder assembles a complete .proof file from inputs.
//
// Per protocol/specs/2026-04-30-proof-file-format.md:
//   1. Build the unsigned body as a CBOR map with sorted keys.
//   2. ed25519-sign the unsigned-body bytes.
//   3. Re-emit with the signature added (keys re-sorted bytewise).
//   4. SHA-256 the final bytes; first 32 hex chars = filename CID.
type Builder struct {
	hasher *canonicalizer.Hasher
}

// NewBuilder returns a fresh Builder.
func NewBuilder() *Builder {
	return &Builder{hasher: canonicalizer.NewHasher()}
}

// Input holds everything Builder.Build needs.
type Input struct {
	Name        string
	Version     string
	Members     map[string][]byte // CID → canonical envelope bytes
	SignerCID   string
	SignerSeed  [32]byte // raw ed25519 seed
	DeclaredAt  string   // RFC 3339, e.g. "2026-04-30T12:00:00.000Z"
}

// Output of Build: bytes of the deterministic-CBOR .proof + filename CID.
type Output struct {
	Bytes       []byte
	FilenameCID string
}

// kvPair is a (key, cbor-encoded-value-blob) pair sorted by bytewise
// CBOR-form of the key per RFC 8949 §4.2.1.
type kvPair struct {
	keyCBOR []byte
	valCBOR []byte
}

func encodeKey(k string) []byte {
	enc := NewCBOREncoder()
	enc.EncodeTStr(k)
	return enc.Bytes()
}

func encodeStringValue(v string) []byte {
	enc := NewCBOREncoder()
	enc.EncodeTStr(v)
	return enc.Bytes()
}

func encodeBStrValue(v []byte) []byte {
	enc := NewCBOREncoder()
	enc.EncodeBStr(v)
	return enc.Bytes()
}

// emitSortedMap writes a CBOR map header + pairs sorted by bytewise
// CBOR-form of each key.
func emitSortedMap(out *CBOREncoder, pairs []kvPair) {
	sort.Slice(pairs, func(i, j int) bool {
		return bytes.Compare(pairs[i].keyCBOR, pairs[j].keyCBOR) < 0
	})
	out.EncodeMapHead(uint64(len(pairs)))
	for _, p := range pairs {
		out.buf = append(out.buf, p.keyCBOR...)
		out.buf = append(out.buf, p.valCBOR...)
	}
}

// encodeMembersMap returns the CBOR bytes for a map of {cid: bstr(envelope)}.
// Member-keys sorted by CBOR-form per §4.2.1.
func encodeMembersMap(members map[string][]byte) []byte {
	pairs := make([]kvPair, 0, len(members))
	for cid, env := range members {
		pairs = append(pairs, kvPair{
			keyCBOR: encodeKey(cid),
			valCBOR: encodeBStrValue(env),
		})
	}
	out := NewCBOREncoder()
	emitSortedMap(out, pairs)
	return out.Bytes()
}

// bodyPairsUnsigned returns the unsigned-body's pairs (everything but the signature).
func bodyPairsUnsigned(in *Input, membersCBOR []byte) []kvPair {
	return []kvPair{
		{keyCBOR: encodeKey("kind"), valCBOR: encodeStringValue("catalog")},
		{keyCBOR: encodeKey("name"), valCBOR: encodeStringValue(in.Name)},
		{keyCBOR: encodeKey("version"), valCBOR: encodeStringValue(in.Version)},
		{keyCBOR: encodeKey("members"), valCBOR: membersCBOR},
		{keyCBOR: encodeKey("signer"), valCBOR: encodeStringValue(in.SignerCID)},
		{keyCBOR: encodeKey("declaredAt"), valCBOR: encodeStringValue(in.DeclaredAt)},
	}
}

// Build assembles the full .proof file. Steps map 1:1 to the spec.
func (b *Builder) Build(in *Input) (*Output, error) {
	membersCBOR := encodeMembersMap(in.Members)

	// 1. Encode unsigned body.
	unsignedPairs := bodyPairsUnsigned(in, membersCBOR)
	unsignedEnc := NewCBOREncoder()
	emitSortedMap(unsignedEnc, unsignedPairs)
	unsignedBytes := unsignedEnc.Bytes()

	// 2. ed25519 sign over the unsigned bytes.
	priv := ed25519.NewKeyFromSeed(in.SignerSeed[:])
	sig := ed25519.Sign(priv, unsignedBytes)

	// 3. Re-emit with the signature added.
	signedPairs := append([]kvPair(nil), unsignedPairs...)
	signedPairs = append(signedPairs, kvPair{
		keyCBOR: encodeKey("signature"),
		valCBOR: encodeBStrValue(sig),
	})
	finalEnc := NewCBOREncoder()
	emitSortedMap(finalEnc, signedPairs)
	finalBytes := finalEnc.Bytes()

	// 4. Filename CID.
	cid := b.hasher.FilenameCID32(finalBytes)
	return &Output{Bytes: finalBytes, FilenameCID: cid}, nil
}
