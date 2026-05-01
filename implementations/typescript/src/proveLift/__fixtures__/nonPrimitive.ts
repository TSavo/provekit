/**
 * Negative fixture: parameter has a non-primitive type. Lift v0 must
 * refuse with `non-primitive-surface`.
 */

export function takesArray(xs: number[]): number {
  return xs.length;
}
