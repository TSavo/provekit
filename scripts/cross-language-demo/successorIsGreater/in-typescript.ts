/**
 * Cross-language demo: TypeScript native source form.
 *
 * Logical claim: for all safe integers x below MAX_SAFE_INTEGER,
 * successor(x) is greater than x.
 *
 * This is ordinary TypeScript source. The TypeScript source lifter reads the
 * function body and emits a function-contract memento whose postcondition
 * records the native expression `return_value = x + 1`.
 *
 * Other host-language equivalents in this directory:
 *   in-rust.rs.example
 *   in-go.go.example
 *   in-cpp.cpp.example
 *
 * The host syntax differs, but each native lifter converges on the same
 * canonical FOL shape once the safe-integer precondition is present.
 */

export function successor(x: number): number {
  if (!Number.isSafeInteger(x) || x >= Number.MAX_SAFE_INTEGER) {
    throw new RangeError("successor is defined for safe integers below MAX_SAFE_INTEGER");
  }
  return x + 1;
}
