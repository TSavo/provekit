/**
 * Circular Proof Demo — TypeScript Final Callee
 *
 * This is the final node in the circular chain.
 * Go calls back into this function via Node-API.
 *
 * Contract: finalizeValue(z: number) → number
 *   Post: output = z * 2
 */

export function finalizeValue(z: number): number {
  // Contract: out = z * 2
  return z * 2;
}
