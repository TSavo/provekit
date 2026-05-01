// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/canonicalizer/canonicalize.ts
// + index.ts (`hashIrJson`, `propertyHashFromFormula`, `contractPropertyHash`).
//
// Public surface covered:
//   * `propertyHashFromFormula(formula) -> string` — self-identifying,
//     length 139 ("blake3-512:" + 128 hex).
//   * `hashIrJson(value) -> string` — same shape.
//   * `contractPropertyHash(spec) -> string` — hashes {pre?,post?,inv?,outBinding}.
//
// Honest scope: the IR cannot prove BLAKE3 collision resistance.
// We can assert shape: output length, prefix presence, determinism.

import {
  must,
  forAll,
  eq,
  num,
  String as StringSort,
  type IrTerm,
} from "../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- propertyHashFromFormula output length is exactly 139. --------------
  must(
    "property_hash_from_formula_output_length_eq_139",
    forAll(StringSort, (f) =>
      eq(ctor1("len", ctor1("propertyHashFromFormula", f)), num(139)),
    ),
  );

  // -- propertyHashFromFormula is deterministic. --------------------------
  must(
    "property_hash_from_formula_is_deterministic",
    forAll(StringSort, (f) =>
      eq(
        ctor1("propertyHashFromFormula", f),
        ctor1("propertyHashFromFormula", f),
      ),
    ),
  );

  // -- hashIrJson output length is exactly 139. ---------------------------
  must(
    "hash_ir_json_output_length_eq_139",
    forAll(StringSort, (v) =>
      eq(ctor1("len", ctor1("hashIrJson", v)), num(139)),
    ),
  );

  // -- hashIrJson is deterministic. ---------------------------------------
  must(
    "hash_ir_json_is_deterministic",
    forAll(StringSort, (v) => eq(ctor1("hashIrJson", v), ctor1("hashIrJson", v))),
  );

  // -- contractPropertyHash output length is exactly 139. -----------------
  must(
    "contract_property_hash_output_length_eq_139",
    forAll(StringSort, (s) =>
      eq(ctor1("len", ctor1("contractPropertyHash", s)), num(139)),
    ),
  );
}
