package verifier

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/provekit/ir-symbolic/canonicalizer"
	"github.com/provekit/ir-symbolic/proof_envelope"
)

// LoadAllProofsStage walks every .proof file under projectRoot
// (project root + node_modules/{*,@*/*}/) and builds the unified
// MementoPool downstream stages hash-look-up against.
//
// Implements §3 rule 1 (filename CID matches content) + §3 rule 2
// (member CIDs match envelope identities). Rule 3 (catalog signature
// verify) is deferred — needs a public-key memento walker that v1
// doesn't yet have.
//
// v1.1.0: every protocol-surface hash is BLAKE3-512 with the
// "blake3-512:" tag; every signature is "ed25519:" + base64(sig).
// Unknown algorithm tags fail-loud per the verifier-dispatch contract
// in protocol/specs/2026-04-30-memento-envelope-grammar.md.
type LoadAllProofsStage struct{}

// Tag prefixes permitted on the v1.1.0 protocol surface.
const (
	hashTagPrefix = "blake3-512:"
	sigTagPrefix  = "ed25519:"
)

// Run loads every .proof in projectRoot and returns the unified pool.
func (s *LoadAllProofsStage) Run(projectRoot string) (*MementoPool, error) {
	pool := &MementoPool{
		Mementos:        map[string]map[string]interface{}{},
		BridgesBySymbol: map[string]map[string]interface{}{},
	}
	for _, pp := range enumerateProofFiles(projectRoot) {
		if err := s.loadOne(pp, pool); err != nil {
			pool.LoadErrors = append(pool.LoadErrors, LoadError{ProofPath: pp, Reason: err.Error()})
		}
	}
	return pool, nil
}

// proofFilenameV11RE matches the v1.1.0 filename shape:
//
//	blake3-512:<128 hex>.proof
var proofFilenameV11RE = regexp.MustCompile(`^(blake3-512:[0-9a-f]{128})\.proof$`)

// proofFilenameAnyTagRE matches any self-identifying-tag filename so
// we can reject unknown tags loud (mirrors C++ load_all_proofs.cpp
// re_anytag).
var proofFilenameAnyTagRE = regexp.MustCompile(`^([a-z0-9]+-[0-9]+:[0-9a-f]+)\.proof$`)

// proofFilenameLegacyRE matches the pre-v1.1.0 sha256 32-hex shape
// (no algorithm tag). Treated as a legacy fixture: filename-shape
// passes (no rule-1 hash check), but the member envelopes inside
// will lack "blake3-512:" prefixes and fail loud at the member-tag
// check below. This mirrors the C++ verifier's permissive filename
// behavior for unmarked files while still hard-rejecting v0 hashes.
var proofFilenameLegacyRE = regexp.MustCompile(`^[0-9a-f]+\.proof$`)

func (s *LoadAllProofsStage) loadOne(proofPath string, pool *MementoPool) error {
	bytes, err := os.ReadFile(proofPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}

	// Rule 1: filename CID matches content (trust root).
	// v1.1.0 prefers `blake3-512:<128 hex>.proof`. We also tolerate
	// bare `<hex>.proof` (legacy path; the hash-tag check below catches
	// any v0 mementos inside it). Any OTHER self-identifying tag is
	// rejected loud — mirrors implementations/cpp/provekit/verifier/
	// load_all_proofs.cpp.
	filename := filepath.Base(proofPath)
	if m := proofFilenameV11RE.FindStringSubmatch(filename); m != nil {
		filenameCID := m[1]
		derived := canonicalizer.ComputeCID(bytes)
		if derived != filenameCID {
			return fmt.Errorf("rule 1 (trust root): filename CID %s != content hash %s",
				filenameCID, derived)
		}
	} else if m := proofFilenameAnyTagRE.FindStringSubmatch(filename); m != nil {
		// Self-identifying tag we do not recognize. Reject loud.
		return fmt.Errorf("rule 1 (trust root): unsupported hash tag in filename %q; v1.1.0 requires `blake3-512:`",
			m[1])
	} else if !proofFilenameLegacyRE.MatchString(filename) {
		// Filename doesn't match any known shape; ignore (don't
		// surface as an error — non-.proof file got into the
		// enumerator? shouldn't happen but be lenient).
	}

	dec := proof_envelope.NewCBORDecoder(bytes)
	catalog, err := dec.DecodeCatalog()
	if err != nil {
		return fmt.Errorf("decode: %w", err)
	}
	membersAny, ok := catalog["members"].(map[string]interface{})
	if !ok {
		return fmt.Errorf("catalog has no `members` map")
	}
	for cid, raw := range membersAny {
		// Tag-dispatch: every member CID MUST be self-identifying.
		// v1.1.0 permits only "blake3-512:"; reject anything else loud.
		if !strings.HasPrefix(cid, hashTagPrefix) {
			pool.LoadErrors = append(pool.LoadErrors, LoadError{
				ProofPath: proofPath,
				Reason:    fmt.Sprintf("member %s: unsupported hash tag; v1.1.0 requires `blake3-512:`", cid),
			})
			continue
		}
		envBytes, ok := raw.([]byte)
		if !ok {
			pool.LoadErrors = append(pool.LoadErrors, LoadError{
				ProofPath: proofPath,
				Reason:    fmt.Sprintf("member %s: value is %T, expected bstr", cid, raw),
			})
			continue
		}
		var env map[string]interface{}
		if err := json.Unmarshal(envBytes, &env); err != nil {
			pool.LoadErrors = append(pool.LoadErrors, LoadError{
				ProofPath: proofPath,
				Reason:    fmt.Sprintf("member %s: parse: %s", cid, err),
			})
			continue
		}
		// Tag-dispatch on producerSignature: v1.1.0 permits "ed25519:" only.
		if sigRaw, ok := env["producerSignature"].(string); ok {
			if !strings.HasPrefix(sigRaw, sigTagPrefix) {
				pool.LoadErrors = append(pool.LoadErrors, LoadError{
					ProofPath: proofPath,
					Reason:    fmt.Sprintf("member %s: unsupported signature tag; v1.1.0 requires `ed25519:`", cid),
				})
				continue
			}
		}
		// Rule 2: re-derive member CID from envelope JCS.
		derived, err := computeEnvelopeCID(env)
		if err != nil || derived != cid {
			pool.LoadErrors = append(pool.LoadErrors, LoadError{
				ProofPath: proofPath,
				Reason:    fmt.Sprintf("rule 2: member %s derives to %s", cid, derived),
			})
			continue
		}
		pool.Mementos[cid] = env
		// If it's a bridge envelope, index by sourceSymbol.
		if ev, ok := env["evidence"].(map[string]interface{}); ok {
			if ev["kind"] == "bridge" {
				if body, ok := ev["body"].(map[string]interface{}); ok {
					if sym, ok := body["sourceSymbol"].(string); ok {
						pool.BridgesBySymbol[sym] = env
					}
				}
			}
		}
	}
	return nil
}

// computeEnvelopeCID re-derives the envelope's CID by JCS-encoding the
// envelope minus cid + producerSignature. Per v1.1.0:
//
//	"blake3-512:" + hex(BLAKE3_512(canonical-bytes))
//
// (universal-claim-envelope.md §CID construction).
func computeEnvelopeCID(env map[string]interface{}) (string, error) {
	stripped := make(map[string]interface{}, len(env))
	for k, v := range env {
		if k == "cid" || k == "producerSignature" {
			continue
		}
		stripped[k] = v
	}
	enc := canonicalizer.NewEncoder()
	bytes, err := enc.Encode(stripped)
	if err != nil {
		return "", err
	}
	return canonicalizer.ComputeCID(bytes), nil
}

// enumerateProofFiles walks projectRoot + node_modules/{*,@*/*}/ for
// every *.proof file (one level deep only).
func enumerateProofFiles(projectRoot string) []string {
	var out []string
	pushProofs := func(dir string) {
		entries, err := os.ReadDir(dir)
		if err != nil {
			return
		}
		for _, e := range entries {
			if !e.IsDir() && strings.HasSuffix(e.Name(), ".proof") {
				out = append(out, filepath.Join(dir, e.Name()))
			}
		}
	}
	pushProofs(projectRoot)

	nodeModules := filepath.Join(projectRoot, "node_modules")
	entries, err := os.ReadDir(nodeModules)
	if err != nil {
		return out
	}
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		if strings.HasPrefix(e.Name(), ".") {
			continue
		}
		entryPath := filepath.Join(nodeModules, e.Name())
		if strings.HasPrefix(e.Name(), "@") {
			scoped, err := os.ReadDir(entryPath)
			if err != nil {
				continue
			}
			for _, sub := range scoped {
				if sub.IsDir() {
					pushProofs(filepath.Join(entryPath, sub.Name()))
				}
			}
		} else {
			pushProofs(entryPath)
		}
	}
	return out
}
