// The canonical encoding-gap fixture: Z3's Real arithmetic and IEEE 754
// runtime disagree on what 0/0 produces. Z3 can assert the quotient is 0;
// the runtime produces NaN. The gap detector should catch the divergence
// and attribute it to line 2.

export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
