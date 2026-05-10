/**
 * Cross-language demo: TypeScript surface form.
 *
 * Logical claim: for all integers x, the successor x+1 is greater than x.
 *
 * This file is the input to the TypeScript kit's lifter. It uses the
 * IR subset described in protocol/specs/2026-04-29-ts-ir-language.md. The
 * lifter produces an IrFormula. The canonicalizer hashes that to a
 * propertyHash.
 *
 * Other host-language equivalents in this directory:
 *   in-rust.invariant.rs.example
 *   in-go.invariant.go.example
 *   in-cpp.invariant.cpp.example
 *
 * The canonical FOL form is the same across all four host languages.
 * The propertyHash is byte-identical. The cross-equivalence is
 * mechanical, not aspirational.
 */

import { property, forAll, type Int } from 'provekit/ir';

property("successorIsGreater",
  forAll<Int>(x => x + 1 > x)
);
