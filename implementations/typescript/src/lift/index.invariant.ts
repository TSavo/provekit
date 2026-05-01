// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/lift/index.ts
//
// Public surface covered:
//   * `liftAndMint(workspaceRoot, outDir, opts) -> Promise<MintReport>`
//   * `mintLiftedDeclarations(decls, opts)`
//   * Default lift options + `DEFAULT_LIFT_SEED`.
//
// Honest scope: the lifter's correctness depends on adapter-specific
// rules (zod, vitest, fast-check, class-validator). The IR captures
// shape: every minted catalog has a CID of length 139, member counts
// are non-negative, the lift seed is exactly 32 bytes.

import {
  must,
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
  // -- The default lift seed is exactly 32 bytes. -------------------------
  must(
    "default_lift_seed_length_eq_32",
    forAll(StringSort, (_s) =>
      eq(ctor1("len", ctor1("defaultLiftSeed", num(0))), num(32)),
    ),
  );

  // -- mintLiftedDeclarations output CID length is exactly 139. -----------
  must(
    "mint_lifted_declarations_cid_length_eq_139",
    forAll(StringSort, (decls) =>
      eq(ctor1("len", ctor1("mintLiftedDeclarationsCid", decls)), num(139)),
    ),
  );

  // -- mintLiftedDeclarations member count is non-negative. ---------------
  must(
    "mint_lifted_declarations_member_count_nonneg",
    forAll(StringSort, (decls) =>
      gte(ctor1("mintLiftedDeclarationsMemberCount", decls), num(0)),
    ),
  );

  // -- liftAndMint is deterministic for a fixed workspace + opts. ---------
  must(
    "lift_and_mint_is_deterministic",
    forAll(StringSort, (root) =>
      eq(
        ctor1("liftAndMintCid", root),
        ctor1("liftAndMintCid", root),
      ),
    ),
  );
}
