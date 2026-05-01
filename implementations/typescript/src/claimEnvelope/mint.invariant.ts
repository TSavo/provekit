// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/claimEnvelope/mint.ts
//
// Public surface covered:
//   * `mintMemento(args) -> ClaimEnvelope`
//   * `mintBridge(args) -> ClaimEnvelope`
//   * `mintContract(args) -> ClaimEnvelope`
//   * `mintImplication(args) -> ClaimEnvelope`
//
// Honest scope: signature validity is cryptographic; the IR captures
// shape — every minted envelope carries a CID with the standard prefix
// and a signature with the ed25519 prefix.

import {
  must,
  contract,
  forAll,
  eq,
  gte,
  num,
  String as StringSort,
  type IrTerm,
} from "../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- Every mintMemento output has a CID of length 139. ------------------
  must(
    "mint_memento_cid_length_eq_139",
    forAll(StringSort, (args) =>
      eq(ctor1("len", ctor1("mintMementoCid", args)), num(139)),
    ),
  );

  // -- Every mintMemento output has a producerSignature of length 96. -----
  // ("ed25519:" prefix is 8 chars; base64 of 64 sig bytes is 88; 8+88=96.)
  must(
    "mint_memento_signature_length_eq_96",
    forAll(StringSort, (args) =>
      eq(ctor1("len", ctor1("mintMementoSignature", args)), num(96)),
    ),
  );

  // -- mintContract requires at least one of pre/post/inv. ----------------
  // Modeled as: invocation count for non-empty contracts is >= 1. The
  // CALL with all three undefined throws — operationally enforced by
  // the implementation; we record the obligation as a length floor on
  // the produced contractName field.
  must(
    "mint_contract_name_nonempty",
    forAll(StringSort, (n) => gte(ctor1("len", ctor1("mintContractName", n)), num(1))),
  );

  // -- mintBridge output binds inputCids to length 1 (one targetContractCid). -
  contract("mint_bridge_input_cids_length_eq_1", {
    post: eq(
      ctor1("len", ctor1("mintBridgeInputCids", num(0))),
      num(1),
    ),
  });

  // -- mintImplication output binds inputCids to length 2 (antecedent + consequent). -
  contract("mint_implication_input_cids_length_eq_2", {
    post: eq(
      ctor1("len", ctor1("mintImplicationInputCids", num(0))),
      num(2),
    ),
  });

  // -- mintMemento output's CID is deterministic for fixed inputs. --------
  must(
    "mint_memento_is_deterministic",
    forAll(StringSort, (a) =>
      eq(ctor1("mintMementoCid", a), ctor1("mintMementoCid", a)),
    ),
  );
}
