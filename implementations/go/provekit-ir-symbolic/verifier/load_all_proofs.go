package verifier

import (
	"crypto/sha256"
	"encoding/hex"
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
type LoadAllProofsStage struct{}

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

var proofFilenameRE = regexp.MustCompile(`^([0-9a-f]+)\.proof$`)

func (s *LoadAllProofsStage) loadOne(proofPath string, pool *MementoPool) error {
	bytes, err := os.ReadFile(proofPath)
	if err != nil {
		return fmt.Errorf("read: %w", err)
	}
	// Rule 1: filename CID matches content.
	filename := filepath.Base(proofPath)
	if m := proofFilenameRE.FindStringSubmatch(filename); m != nil {
		sum := sha256.Sum256(bytes)
		derived := hex.EncodeToString(sum[:])[:32]
		if derived != m[1] {
			return fmt.Errorf("rule 1: filename CID %s != content hash %s", m[1], derived)
		}
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
// envelope minus cid + producerSignature. This is the spec's CID rule
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
	return canonicalizer.NewHasher().EnvelopeCID32(bytes), nil
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
