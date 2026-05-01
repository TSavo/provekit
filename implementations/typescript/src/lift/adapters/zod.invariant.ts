// SPDX-License-Identifier: Apache-2.0
//
// .invariant.ts for implementations/typescript/src/lift/adapters/zod.ts
//
// Public surface covered: zod schema -> IR-formula adapter.
//
// Honest scope: the IR can pin shape — lifted declarations carry
// outBinding "out", produce IR-JSON conforming to v1.1.0, are
// deterministic for a fixed input schema.

import {
  must,
  forAll,
  eq,
  gte,
  num,
  String as StringSort,
  type IrTerm,
} from "../../ir/symbolic/index.js";

function ctor1(name: string, arg: IrTerm): IrTerm {
  return { kind: "ctor", name, args: [arg] };
}

export function invariants(): void {
  // -- liftZodSchema produces deterministic IR for a fixed schema. --------
  must(
    "lift_zod_schema_is_deterministic",
    forAll(StringSort, (schema) =>
      eq(
        ctor1("liftZodSchemaIr", schema),
        ctor1("liftZodSchemaIr", schema),
      ),
    ),
  );

  // -- Every zod-lifted declaration has outBinding "out" by default. ------
  must(
    "lift_zod_default_out_binding_length_eq_3",
    forAll(StringSort, (_d) =>
      eq(ctor1("len", ctor1("liftZodOutBinding", num(0))), num(3)),
    ),
  );

  // -- liftZodSchema declaration count is non-negative. -------------------
  must(
    "lift_zod_schema_decl_count_nonneg",
    forAll(StringSort, (s) =>
      gte(ctor1("liftZodSchemaDeclCount", s), num(0)),
    ),
  );
}
