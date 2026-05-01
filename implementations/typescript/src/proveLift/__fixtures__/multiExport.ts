/**
 * Negative fixture: more than one exported function. Lift v0 must
 * refuse with `multiple-exports`.
 */

export function alpha(s: string): number {
  return s.length;
}

export function beta(n: number): boolean {
  return n > 0;
}
