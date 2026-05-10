// @ts-nocheck
/**
 * Mixed fixture for all three TS lift adapters: zod + fast-check +
 * class-validator. Exercises every adapter on a single file so the
 * vitest plugin's end-to-end pipeline can mint a single .proof catalog
 * containing contracts from all three sources.
 *
 * NOTE: This file is a LIFT TARGET, not executed at test time. We use
 * `// @ts-nocheck` and type-only imports so the project doesn't need
 * zod / class-validator as direct dependencies. The lifter walks the
 * AST and never calls into these libraries' runtimes.
 */

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore: zod consumed AST-only.
import { z } from "zod";
import * as fc from "fast-check";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore: class-validator consumed AST-only.
import {
  IsNotEmpty,
  MinLength,
  MaxLength,
  IsEmail,
  IsInt,
  Min,
  Max,
  IsBoolean,
  IsUUID,
} from "class-validator";

// ---------------------------------------------------------------------------
// zod (3 lifted)
// ---------------------------------------------------------------------------

export const ZodUser = z.object({
  email: z.string().email(),
  age: z.number().int().nonnegative(),
});

export const ZodPort = z.number().int().min(1).max(65535);

export const ZodName = z.string().nonempty();

// ---------------------------------------------------------------------------
// fast-check (2 lifted)
// ---------------------------------------------------------------------------

it("fc addition commutative", () => {
  fc.assert(fc.property(fc.integer(), fc.integer(), (a, b) => a + b === b + a));
});

it("fc nat is non-negative", () => {
  fc.assert(fc.property(fc.nat(), (n) => n >= 0));
});

// ---------------------------------------------------------------------------
// class-validator (3 lifted, 1 skipped)
// ---------------------------------------------------------------------------

export class CreateUserDto {
  @IsNotEmpty()
  @MinLength(2)
  @MaxLength(64)
  username: string;

  @IsEmail()
  email: string;

  @IsInt()
  @Min(0)
  @Max(120)
  age: number;

  @IsBoolean()
  active: boolean;
}

export class TokenDto {
  @IsUUID()
  @IsNotEmpty()
  id: string;
}

export class CreateProductDto {
  @MinLength(3)
  @MaxLength(64)
  sku: string;

  @Min(0)
  price: number;
}

/** Skipped: contains an unsupported decorator (custom validator). */
export class UnsupportedDto {
  @IsNotEmpty()
  // @ts-ignore: fictional custom decorator the adapter must skip on.
  @CustomMagicCheck()
  payload: string;
}
