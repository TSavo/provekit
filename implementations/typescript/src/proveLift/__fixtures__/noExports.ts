/**
 * Negative fixture: no exports. Lift v0 must refuse with `no-exports`.
 */

function helper(s: string): number {
  return s.length;
}

void helper;
