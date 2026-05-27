// TypeScript consumer of the C++ divide library.
//
// In a real project, this would be loaded via N-API or wasm-bindgen.
// For the demo, the import is illustrative.
//
// Native source contract surface:
// - the denominator guard is ordinary TypeScript
// - the TS source lifter derives the wrapper's function-contract memento
//   from this function body, with no sibling contract file

import { divide } from "@cpp-libs/divide";  // illustrative import

export function safeDivide(numerator: number, denominator: number): number {
  if (denominator === 0) {
    throw new Error("denominator must not be zero");
  }
  return divide(numerator, denominator);
}

export function unsafeUsage(): number {
  // This call site has no upstream guard; verification will reject.
  const userInput = process.argv[2];
  return divide(100, parseInt(userInput));
}
