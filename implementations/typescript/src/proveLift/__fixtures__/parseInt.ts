/**
 * Test fixture: a real-world parseInt implementation with no
 * hand-authored invariants. The lift adapter must derive the
 * `forall n: Int. n >= 0 -> parseInt(String(n)) === n` precondition
 * from this file alone.
 *
 * The eventual minted property memento must be CID-equivalent to
 * scripts/output/parseInt-mementos/parseIntPreservesNonNegativeIntegers.json
 * (propertyHash 8c38f05152707736).
 */

export function parseInt(s: string): number {
  // Real implementation is irrelevant to lift; what matters is the
  // signature and the test corpus. Lift v0 reasons from the type
  // signature; richer body analysis is roadmap.
  const n = Number(s);
  if (Number.isNaN(n)) return NaN;
  return Math.trunc(n);
}
