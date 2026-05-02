/**
 * Circular Proof Demo — TypeScript Caller
 * 
 * This TypeScript module calls a C++ function via WASM.
 * The contract ensures: input ≥ 0 → output ≥ input
 */

// Bridge to C++ function: multiply2x(x: number) → number
// The C++ function is compiled to WASM and loaded here.
declare function multiply2x(x: number): number;

// Entry point: processValue
// Contract: if input ≥ 0, then output ≥ input
export function processValue(input: number): number {
  // Precondition checked by contract
  if (input < 0) {
    throw new Error("Precondition violated: input must be ≥ 0");
  }
  
  // Bridge call to C++
  const result = multiply2x(input);
  
  // Postcondition: result ≥ input (since C++ guarantees 2x)
  return result;
}
