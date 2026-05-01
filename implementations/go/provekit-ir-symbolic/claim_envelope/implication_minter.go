package claim_envelope

import "fmt"

// ImplicationMintArgs is the input to (*Minter).MintImplication.
//
// AntecedentSlot / ConsequentSlot are the slot names ("pre" / "post" /
// "inv") naming which formula in each contract memento participates.
//
// SmtLibInput / ProofWitness are optional; "" → field omitted.
//
// bindingHash and propertyHash are DERIVED.
//
// AntecedentHash / ConsequentHash are full BLAKE3-512 CIDs in v1.1.0
// self-identifying form ("blake3-512:" + 128 hex chars).
type ImplicationMintArgs struct {
	ProducedBy     string
	ProducedAt     string
	AntecedentHash string // "blake3-512:" + 128 hex
	ConsequentHash string // "blake3-512:" + 128 hex
	AntecedentCID  string // contract memento CID
	ConsequentCID  string // contract memento CID
	AntecedentSlot string // "pre" | "post" | "inv"
	ConsequentSlot string // "pre" | "post" | "inv"
	Prover         string // e.g. "z3@4.13.4"
	ProverRunMs    int64
	SmtLibInput    string // optional
	ProofWitness   string // optional
}

// MintImplication builds + signs a v1.1.0 implication ClaimEnvelope.
//
// Implication mementos are how the handshake algorithm caches proven
// facts: once any party has discharged `forall x. Q(x) -> P(x)`, the
// witness is content-addressed and shared. Future verifiers hit the
// cache (Tier 2) instead of re-running the solver (Tier 3).
//
// Per spec (memento envelope grammar §Role: ImplicationMemento), v1.1.0
// full-BLAKE3-512 self-identifying form:
//
//	bindingHash  = ComputeCID(canonical({antecedentHash, consequentHash}))
//	propertyHash = ComputeCID("implication:" || antecedentHash || ":" || consequentHash)
//	inputCids    = [antecedentCid, consequentCid] (lex-sorted by envelopeForHashing)
func (m *Minter) MintImplication(args ImplicationMintArgs) (*Minted, error) {
	if args.AntecedentHash == "" || args.ConsequentHash == "" {
		return nil, fmt.Errorf("MintImplication: antecedent + consequent hashes are required")
	}
	if args.AntecedentCID == "" || args.ConsequentCID == "" {
		return nil, fmt.Errorf("MintImplication: antecedent + consequent CIDs are required")
	}

	body := map[string]interface{}{
		"antecedentHash": args.AntecedentHash,
		"consequentHash": args.ConsequentHash,
		"antecedentCid":  args.AntecedentCID,
		"consequentCid":  args.ConsequentCID,
		"antecedentSlot": args.AntecedentSlot,
		"consequentSlot": args.ConsequentSlot,
		"prover":         args.Prover,
		"proverRunMs":    args.ProverRunMs,
	}
	if args.SmtLibInput != "" {
		body["smtLibInput"] = args.SmtLibInput
	}
	if args.ProofWitness != "" {
		body["proofWitness"] = args.ProofWitness
	}
	evidence := map[string]interface{}{
		"kind":   "implication",
		"schema": SchemaCIDImplication,
		"body":   body,
	}

	bindingHash, err := hashValue(map[string]interface{}{
		"antecedentHash": args.AntecedentHash,
		"consequentHash": args.ConsequentHash,
	})
	if err != nil {
		return nil, fmt.Errorf("MintImplication: bindingHash: %w", err)
	}
	propertyHash := hashRawString("implication:" + args.AntecedentHash + ":" + args.ConsequentHash)

	unsigned := envelopeForHashing(
		bindingHash, propertyHash, VerdictHolds,
		args.ProducedBy, args.ProducedAt,
		[]string{args.AntecedentCID, args.ConsequentCID},
		evidence,
	)
	return m.finalize(unsigned)
}
