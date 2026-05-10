// @ts-nocheck
/**
 * Mixed fixture: zod schemas + fast-check properties.
 *
 * The provekit-lift TS adapters lift each top-level z.<schema>
 * declaration and each fc.assert(fc.property(...)) test into a
 * ContractDecl, which mints to a contract memento, all bundled into a
 * single signed `.proof`.
 *
 * NOTE: This file is a LIFT TARGET, not executed at test time. We use a
 * type-only import for `zod` so the project doesn't require zod as a
 * direct dependency (zod is only available transitively via SDKs in
 * pnpm-land). The shape of the AST is what matters; the lifter walks
 * it and never calls into zod's runtime.
 */

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore: zod is consumed AST-only by the lifter; not executed here.
import { z } from "zod";
import * as fc from "fast-check";

// ---------------------------------------------------------------------------
// zod schemas (5 lifted, 1 skipped with warning)
// ---------------------------------------------------------------------------

export const UserSchema = z.object({
  age: z.number().int().nonnegative(),
  email: z.string().min(1).email(),
});

export const ProductSchema = z.object({
  sku: z.string().min(3).max(64),
  price: z.number().positive(),
});

export const PortSchema = z.number().int().min(1).max(65535);

export const UuidSchema = z.string().uuid();

export const NameSchema = z.string().nonempty();

/** Skipped: arbitrary callback in .refine: lifter logs and bails. */
export const CustomCheckedString = z
  .string()
  .refine((s) => s.indexOf("@") >= 0, { message: "must contain @" });

// ---------------------------------------------------------------------------
// fast-check properties (3 lifted, 1 skipped)
// ---------------------------------------------------------------------------

it("addition is commutative", () => {
  fc.assert(fc.property(fc.integer(), fc.integer(), (a, b) => a + b === b + a));
});

it("identity squared equals identity", () => {
  fc.assert(fc.property(fc.integer(), (x) => x * 1 === x));
});

it("nat is non-negative", () => {
  fc.assert(fc.property(fc.nat(), (n) => n >= 0));
});

/** Skipped: predicate body is not a top-level comparison (multi-statement). */
it("complex body skipped", () => {
  fc.assert(
    fc.property(fc.integer(), (x) => {
      const y = x * 2;
      const z = y + 1;
      return z > x;
    }),
  );
});
