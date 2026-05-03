// SPDX-License-Identifier: Apache-2.0
//
// contract_set_cid.go — spec #94 contractSetCid helpers.
//
// Two functions:
//
//   ContractCIDFromArgs(args ContractMintArgs) string
//       Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
//       contractCid := blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
//       Signer-independent. Two distinct signers attesting to the same
//       logical contract produce the same contractCid.
//
//   ComputeContractSetCID(contractCIDs []string) string
//       Per spec 2026-05-03-contract-set-extension.md §1:
//       contractSetCid := blake3-512(JCS(<sorted contractCIDs>))
//       Sort is lexicographic on the raw "blake3-512:hex" strings.

package claim_envelope

import (
	"sort"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

// ContractCIDFromArgs computes the signer-independent contractCid for a
// contract. Per spec 2026-05-03-contract-cid-vs-attestation-cid.md §1:
//
//	contractCid := "blake3-512:" || hex(BLAKE3-512(JCS({name, outBinding, pre?, post?, inv?})))
//
// Two distinct signers attesting to the same logical contract produce the
// same contractCid. This is NOT the attestation CID (envelope hash).
func ContractCIDFromArgs(args ContractMintArgs) (string, error) {
	enc := canonicalizer.NewEncoder()
	obj := map[string]interface{}{
		"name":       args.ContractName,
		"outBinding": args.OutBinding,
	}
	if args.Pre != nil {
		obj["pre"] = args.Pre
	}
	if args.Post != nil {
		obj["post"] = args.Post
	}
	if args.Inv != nil {
		obj["inv"] = args.Inv
	}
	b, err := enc.Encode(obj)
	if err != nil {
		return "", err
	}
	return canonicalizer.ComputeCID(b), nil
}

// ComputeContractSetCID computes the contract set CID from a slice of
// signer-independent contractCid strings (each "blake3-512:<128 hex>").
//
// Per spec 2026-05-03-contract-set-extension.md §1:
//
//	contractSetCid := "blake3-512:" || hex(BLAKE3-512(JCS(<sorted contractCIDs>)))
//
// The sort is lexicographic on the raw strings, making the result
// order-independent: two kits enumerating the same contracts in different
// order produce byte-identical contractSetCid values.
func ComputeContractSetCID(contractCIDs []string) (string, error) {
	sorted := make([]string, len(contractCIDs))
	copy(sorted, contractCIDs)
	sort.Strings(sorted)

	arr := make([]interface{}, len(sorted))
	for i, c := range sorted {
		arr[i] = c
	}

	enc := canonicalizer.NewEncoder()
	b, err := enc.Encode(arr)
	if err != nil {
		return "", err
	}
	return canonicalizer.ComputeCID(b), nil
}
